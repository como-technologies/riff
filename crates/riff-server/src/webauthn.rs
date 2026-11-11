//! The check of the reply of a FIDO2 / WebAuthn authenticator: the add
//! of a key of a person (01M75NH2VF6HGZVY04GB358JG0), and the assertion
//! of a proof of presence ([`check_assertion`], 01M75WKS1TS9FME704FQNBZQN1).
//!
//! # Design
//!
//! riff-server is the relying party ([`Party`]). Its ID is the host of
//! the public URL, and a loopback address gives `localhost`
//! (01M75NH2Y3H1CR10CGRWNGT8RG). [`options`] makes the options of a new
//! credential: a new challenge, the algorithms ES256, EdDSA and RS256,
//! no attestation, and the keys that the person has, so that an
//! authenticator makes no second key. The page of the add gives them to
//! `navigator.credentials.create`, and sends the reply back
//! ([`crate::devices`]). [`check`] checks the reply:
//!
//! ```mermaid
//! flowchart TD
//!     R[the reply of the page] --> C{clientDataJSON: type webauthn.create,<br/>the challenge of the add, the origin of riff-server}
//!     C -->|no| X[refused, with the reason]
//!     C --> A{authenticator data: the hash of the party ID,<br/>user presence, a credential}
//!     A -->|no| X
//!     A --> K{the COSE key: ES256, EdDSA or RS256;<br/>the rawId is its credential ID}
//!     K -->|no| X
//!     K --> F{the attestation}
//!     F -->|none| G[the key]
//!     F -->|packed, no certificate| S{the signature of the new key over<br/>the data and the hash of clientDataJSON}
//!     S -->|good| G
//!     S -->|bad| X
//!     F -->|each other form| X
//! ```
//!
//! - riff asks for no attestation, so a browser gives the form `none`,
//!   or `packed` with no certificate: the self attestation of an
//!   authenticator. riff trusts no maker of authenticators, so a
//!   certificate would prove nothing to it. It refuses each other form,
//!   and says why.
//! - [`PublicKey`] reads the COSE key of the credential, and checks a
//!   signature with it. The record `key_added` keeps the COSE key as it
//!   came, so a later proof checks with the same key.
//! - Each refusal is a text for a person ([`Refused`]). It names no
//!   secret: the challenge is no secret, and the reply holds none.

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;
use ciborium::Value;
use p256::ecdsa::signature::Verifier;
use riff_core::wire::{
    KeyAlgorithm, KeyCredential, KeyOptions, KeyParty, KeyReply, KeySelection, KeyUser,
    ProofOptions, ProofReply,
};
use riff_state::state::PersonKey;
use serde::Deserialize;
use sha2::{Digest, Sha256};

/// The COSE number of ES256: ECDSA with P-256 and SHA-256.
pub const ES256: i64 = -7;
/// The COSE number of EdDSA, with Ed25519.
pub const EDDSA: i64 = -8;
/// The COSE number of RS256: RSASSA-PKCS1-v1_5 with SHA-256.
pub const RS256: i64 = -257;

/// The algorithms that riff-server asks for, the first one first.
pub const ALGORITHMS: [i64; 3] = [ES256, EDDSA, RS256];

/// How long the browser waits for the authenticator, in milliseconds.
pub const TIMEOUT_MS: u64 = 5 * 60 * 1000;

/// The longest credential ID of WebAuthn, in bytes.
pub const CREDENTIAL_MOST: usize = 1023;

/// The flag of user presence in the authenticator data.
const PRESENT: u8 = 0x01;
/// The flag of attested credential data.
const ATTESTED: u8 = 0x40;

/// How deep a CBOR value of a reply can nest.
const DEPTH: usize = 16;

/// Why riff-server refuses a reply: a text for a person.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Refused(pub String);

impl std::fmt::Display for Refused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

fn refused<T>(reason: impl Into<String>) -> Result<T, Refused> {
    Err(Refused(reason.into()))
}

/// riff-server as the relying party of WebAuthn
/// (01M75NH2Y3H1CR10CGRWNGT8RG): its ID, and the origin of its pages.
///
/// ```
/// use riff_server::webauthn::Party;
///
/// let cloud = Party::of("https://riff.example.com");
/// assert_eq!(cloud.id, "riff.example.com");
/// assert_eq!(cloud.origin, "https://riff.example.com");
///
/// // A browser takes no IP address as a relying party ID.
/// let local = Party::of("http://127.0.0.1:7878");
/// assert_eq!(local.id, "localhost");
/// assert_eq!(local.origin, "http://localhost:7878");
/// assert_eq!(Party::of("http://[::1]:7878").origin, "http://localhost:7878");
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Party {
    /// The relying party ID: a host name.
    pub id: String,
    /// The origin of the pages of riff-server: the scheme, the host and
    /// the port.
    pub origin: String,
}

impl Party {
    /// The party of the public URL `public_url`. A URL that does not
    /// read gives its text as the origin, with no ID: each check then
    /// fails.
    pub fn of(public_url: &str) -> Party {
        let Ok(url) = reqwest::Url::parse(public_url) else {
            return Party {
                id: String::new(),
                origin: public_url.to_owned(),
            };
        };
        let host = url.host_str().unwrap_or_default();
        let loopback = host
            .trim_start_matches('[')
            .trim_end_matches(']')
            .parse::<std::net::IpAddr>()
            .is_ok_and(|ip| ip.is_loopback());
        let host = match loopback {
            true => "localhost".to_owned(),
            false => host.to_owned(),
        };
        let origin = match url.port() {
            Some(port) => format!("{}://{host}:{port}", url.scheme()),
            None => format!("{}://{host}", url.scheme()),
        };
        Party { id: host, origin }
    }
}

/// The options of a new credential for the add of the key `name` of
/// `user` (01M75NH2VF6HGZVY04GB358JG0). `handle` is the user handle: the
/// same for each key of the person. `credentials` are the credential
/// IDs of the keys that the person has, in base64url.
///
/// ```
/// use riff_server::webauthn::{ALGORITHMS, Party, options};
///
/// let party = Party::of("https://riff.example.com");
/// let options = options(&party, "mike", b"handle", b"challenge", &["Y3JlZA".into()], "yubikey");
/// assert_eq!(options.rp.id, "riff.example.com");
/// assert_eq!(options.attestation, "none");
/// let algorithms: Vec<i64> = options.pub_key_cred_params.iter().map(|p| p.alg).collect();
/// assert_eq!(algorithms, ALGORITHMS);
/// assert_eq!(options.exclude_credentials[0].id, "Y3JlZA");
/// ```
pub fn options(
    party: &Party,
    user: &str,
    handle: &[u8],
    challenge: &[u8],
    credentials: &[String],
    name: &str,
) -> KeyOptions {
    let public_key = || "public-key".to_owned();
    KeyOptions {
        rp: KeyParty {
            id: party.id.clone(),
            name: "riff".into(),
        },
        user: KeyUser {
            id: B64.encode(handle),
            name: user.to_owned(),
            display_name: user.to_owned(),
        },
        challenge: B64.encode(challenge),
        pub_key_cred_params: ALGORITHMS
            .iter()
            .map(|&alg| KeyAlgorithm {
                kind: public_key(),
                alg,
            })
            .collect(),
        timeout: TIMEOUT_MS,
        exclude_credentials: credentials
            .iter()
            .map(|id| KeyCredential {
                kind: public_key(),
                id: id.clone(),
            })
            .collect(),
        authenticator_selection: KeySelection {
            resident_key: "discouraged".into(),
            user_verification: "preferred".into(),
        },
        attestation: "none".into(),
        name: name.to_owned(),
    }
}

/// A key that [`check`] took: the credential ID and the COSE key, as
/// the record `key_added` keeps them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Checked {
    pub credential: Vec<u8>,
    /// The COSE key, as the authenticator gave it.
    pub public_key: Vec<u8>,
    /// The COSE number of the algorithm of the key.
    pub alg: i64,
}

/// The `clientDataJSON` of a reply: the fields that riff checks.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ClientData {
    #[serde(rename = "type")]
    kind: String,
    challenge: String,
    origin: String,
    #[serde(default)]
    cross_origin: bool,
}

/// Checks the reply of a new credential for the add with the challenge
/// `challenge` on `party` (01M75NH2VF6HGZVY04GB358JG0). See the module
/// docs for each check. Gives the key, or why riff refuses it.
///
/// ```
/// use base64::Engine;
/// use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;
/// use riff_core::device::fake::{Authenticator, Shape};
/// use riff_server::webauthn::{Party, check, options};
///
/// let party = Party::of("https://riff.example.com");
/// let options = options(&party, "mike", b"handle", b"challenge", &[], "yubikey");
/// let key = Authenticator::new();
/// let reply = key.reply("T", &options, &party.origin);
/// let checked = check(&party, b"challenge", &reply).unwrap();
/// assert_eq!(B64.encode(&checked.credential), key.credential());
///
/// let bad = key.reply_as("T", &options, &party.origin, Shape::BadSignature);
/// assert!(check(&party, b"challenge", &bad).unwrap_err().0.contains("signature"));
/// ```
pub fn check(party: &Party, challenge: &[u8], reply: &KeyReply) -> Result<Checked, Refused> {
    let decode = |what: &str, text: &str| {
        B64.decode(text)
            .map_err(|_| Refused(format!("the {what} of the reply is not base64url")))
    };
    let raw_id = decode("rawId", &reply.raw_id)?;
    let client_json = decode("clientDataJSON", &reply.client_data)?;
    let attestation = decode("attestationObject", &reply.attestation)?;

    let client: ClientData = serde_json::from_slice(&client_json).map_err(|e| {
        Refused(format!(
            "the clientDataJSON of the reply does not read: {e}"
        ))
    })?;
    if client.kind != "webauthn.create" {
        return refused(format!(
            "the reply has the type {:?}, not webauthn.create: it is no new credential",
            client.kind
        ));
    }
    if B64.decode(&client.challenge).ok().as_deref() != Some(challenge) {
        return refused("the reply has a challenge that riff-server did not give");
    }
    if client.origin != party.origin || client.cross_origin {
        return refused(format!(
            "the reply comes from {}, not from the page of riff-server at {}",
            client.origin, party.origin
        ));
    }

    let object = read_cbor(&attestation, "attestationObject")?;
    let form = field(&object, "fmt").and_then(Value::as_text);
    let statement = field(&object, "attStmt").and_then(Value::as_map);
    let data = field(&object, "authData").and_then(Value::as_bytes);
    let (Some(form), Some(statement), Some(data)) = (form, statement, data) else {
        return refused("the attestationObject of the reply has no fmt, attStmt or authData");
    };

    let credential = Credential::read(data)?;
    if credential.party != Sha256::digest(party.id.as_bytes()).as_slice() {
        return refused(format!(
            "the authenticator made the key for another relying party, not for {}",
            party.id
        ));
    }
    if credential.flags & PRESENT == 0 {
        return refused("the authenticator saw no touch: the reply has no user presence");
    }
    if credential.id != raw_id {
        return refused("the rawId of the reply is not the credential of the authenticator data");
    }
    let key = PublicKey::from_cose(&credential.public_key)?;

    match form {
        "none" if statement.is_empty() => {}
        "none" => return refused("the attestation none of the reply has a statement"),
        "packed" => {
            if field_of(statement, "x5c").is_some() {
                return refused(
                    "the reply has an attestation with a certificate: riff asks for none. \
                     Try again, or try another browser",
                );
            }
            let alg = field_of(statement, "alg").and_then(integer);
            let sig = field_of(statement, "sig").and_then(Value::as_bytes);
            let (Some(alg), Some(sig)) = (alg, sig) else {
                return refused("the attestation packed of the reply has no alg or sig");
            };
            if alg != key.alg() {
                return refused("the attestation of the reply has another algorithm than its key");
            }
            let mut signed = data.clone();
            signed.extend_from_slice(&Sha256::digest(&client_json));
            if !key.verify(&signed, sig) {
                return refused("the signature of the reply does not check with its new key");
            }
        }
        other => {
            return refused(format!(
                "the reply has the attestation {other}: riff takes only none and packed. \
                 Try again, or try another browser"
            ));
        }
    }
    Ok(Checked {
        credential: credential.id,
        alg: key.alg(),
        public_key: credential.public_key,
    })
}

/// A proof that [`check_assertion`] took: the name of the key that signed, and
/// the counter of the authenticator in the signed data.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Asserted {
    pub name: String,
    pub counter: u64,
}

/// Checks the assertion of a proof of presence with the challenge
/// `challenge` on `party` (01M75WKS1TS9FME704FQNBZQN1): the type `webauthn.get`, the
/// challenge, the origin of riff-server, the hash of the relying party
/// ID, the flag of user presence, a credential of `keys`, and the
/// signature of that key over the authenticator data and the hash of
/// `clientDataJSON`. `keys` are the keys of the person, by name. The
/// state checks the counter (`riff_state::state::Prove`).
///
/// ```mermaid
/// flowchart TD
///     R[the assertion of the page] --> C{clientDataJSON: type webauthn.get,<br/>the challenge of the proof, the origin of riff-server}
///     C -->|no| X[refused, with the reason]
///     C --> A{authenticator data: the hash of the party ID,<br/>user presence}
///     A -->|no| X
///     A --> K{the rawId is a credential<br/>of a key of the person}
///     K -->|no| X
///     K --> S{the signature checks with that key}
///     S -->|no| X
///     S --> G[the name of the key, and the counter]
/// ```
///
/// ```
/// use riff_core::device::fake::{Authenticator, Proof, proof_options};
/// use riff_server::webauthn::{Party, check_assertion};
/// use riff_state::state::PersonKey;
///
/// let party = Party::of("https://riff.example.com");
/// let key = Authenticator::new();
/// let kept = PersonKey { credential: key.credential(), public_key: key.public_key(), added_ms: 0, used_ms: 0, counter: 0 };
/// let keys = [("yubikey".to_owned(), kept)];
/// // The challenge "c2VlZA" is the base64url of "seed".
/// let options = proof_options(&party.id, "c2VlZA", &key);
/// let good = key.assert("T", &options, &party.origin);
/// let asserted = check_assertion(&party, b"seed", &good, &keys).unwrap();
/// assert_eq!((asserted.name.as_str(), asserted.counter), ("yubikey", 1));
///
/// let bad = key.assert_as("T", &options, &party.origin, Proof::BadSignature);
/// assert!(check_assertion(&party, b"seed", &bad, &keys).unwrap_err().0.contains("signature"));
/// ```
pub fn check_assertion(
    party: &Party,
    challenge: &[u8],
    reply: &ProofReply,
    keys: &[(String, PersonKey)],
) -> Result<Asserted, Refused> {
    let decode = |what: &str, text: &str| {
        B64.decode(text)
            .map_err(|_| Refused(format!("the {what} of the proof is not base64url")))
    };
    let raw_id = decode("rawId", &reply.raw_id)?;
    let client_json = decode("clientDataJSON", &reply.client_data)?;
    let data = decode("authenticatorData", &reply.authenticator_data)?;
    let signature = decode("signature", &reply.signature)?;

    let client: ClientData = serde_json::from_slice(&client_json).map_err(|e| {
        Refused(format!(
            "the clientDataJSON of the proof does not read: {e}"
        ))
    })?;
    if client.kind != "webauthn.get" {
        return refused(format!(
            "the proof has the type {:?}, not webauthn.get: it is no signature of a key",
            client.kind
        ));
    }
    if B64.decode(&client.challenge).ok().as_deref() != Some(challenge) {
        return refused("the proof has a challenge that riff-server did not give for it");
    }
    if client.origin != party.origin || client.cross_origin {
        return refused(format!(
            "the proof comes from {}, not from the page of riff-server at {}",
            client.origin, party.origin
        ));
    }
    if data.len() < 37 {
        return refused("the authenticator data of the proof is too short");
    }
    if data[..32] != Sha256::digest(party.id.as_bytes())[..] {
        return refused(format!(
            "the authenticator signed for another relying party, not for {}",
            party.id
        ));
    }
    if data[32] & PRESENT == 0 {
        return refused("the authenticator saw no touch: the proof has no user presence");
    }
    let counter = u64::from(u32::from_be_bytes([data[33], data[34], data[35], data[36]]));
    let credential = B64.encode(&raw_id);
    let Some((name, key)) = keys.iter().find(|(_, key)| key.credential == credential) else {
        return refused(
            "the authenticator signed with a key that is not one of yours: riff device list \
             shows your keys",
        );
    };
    let cose = B64.decode(&key.public_key).map_err(|_| {
        Refused(format!(
            "riff cannot read the public key of your key {name}"
        ))
    })?;
    let public_key = PublicKey::from_cose(&cose)?;
    let mut signed = data.clone();
    signed.extend_from_slice(&Sha256::digest(&client_json));
    if !public_key.verify(&signed, &signature) {
        return refused(format!(
            "the signature of the proof does not check with your key {name}"
        ));
    }
    Ok(Asserted {
        name: name.clone(),
        counter,
    })
}

/// The options of the assertion of a proof (01M75WKC8RCPHQW3NADHRA31SG): the challenge,
/// the relying party, the credential of each key of the person, and the
/// text of the action for the page.
pub fn proof_options(
    party: &Party,
    user: &str,
    challenge: &[u8],
    credentials: &[String],
    action: &str,
) -> ProofOptions {
    ProofOptions {
        challenge: B64.encode(challenge),
        timeout: TIMEOUT_MS,
        rp_id: party.id.clone(),
        allow_credentials: credentials
            .iter()
            .map(|id| KeyCredential {
                kind: "public-key".into(),
                id: id.clone(),
            })
            .collect(),
        user_verification: "preferred".into(),
        action: action.to_owned(),
        user: user.to_owned(),
    }
}

/// The parts of the authenticator data of a new credential.
struct Credential {
    /// The SHA-256 hash of the relying party ID.
    party: Vec<u8>,
    flags: u8,
    id: Vec<u8>,
    /// The COSE key, as its CBOR bytes.
    public_key: Vec<u8>,
}

impl Credential {
    /// Reads the authenticator data: the hash of the party (32 bytes),
    /// the flags (1), the counter (4), then the AAGUID (16), the length
    /// of the credential ID (2), the ID, and the COSE key. Extensions
    /// can follow the key.
    fn read(data: &[u8]) -> Result<Credential, Refused> {
        let short = || Refused("the authenticator data of the reply is too short".into());
        let party = data.get(..32).ok_or_else(short)?.to_vec();
        let flags = *data.get(32).ok_or_else(short)?;
        if flags & ATTESTED == 0 {
            return refused("the authenticator data of the reply has no credential");
        }
        let length = data.get(53..55).ok_or_else(short)?;
        let length = usize::from(u16::from_be_bytes([length[0], length[1]]));
        if length == 0 || length > CREDENTIAL_MOST {
            return refused("the credential ID of the reply has a length that is not allowed");
        }
        let id = data.get(55..55 + length).ok_or_else(short)?.to_vec();
        let rest = &data[55 + length..];
        let mut reader = rest;
        ciborium::de::from_reader_with_recursion_limit::<Value, _>(&mut reader, DEPTH)
            .map_err(|_| Refused("the COSE key of the reply does not read".into()))?;
        let public_key = rest[..rest.len() - reader.len()].to_vec();
        Ok(Credential {
            party,
            flags,
            id,
            public_key,
        })
    }
}

/// A public key of a credential, from its COSE key: ES256, EdDSA or
/// RS256.
///
/// ```
/// use base64::Engine;
/// use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;
/// use riff_core::device::fake::Authenticator;
/// use riff_server::webauthn::{ES256, PublicKey};
///
/// let cose = B64.decode(Authenticator::new().public_key()).unwrap();
/// assert_eq!(PublicKey::from_cose(&cose).unwrap().alg(), ES256);
/// assert!(PublicKey::from_cose(b"\xa0").is_err());
/// ```
#[derive(Clone, Debug)]
pub enum PublicKey {
    Es256(p256::ecdsa::VerifyingKey),
    /// The 32 bytes of an Ed25519 key.
    EdDsa(Vec<u8>),
    /// The modulus and the exponent, big-endian.
    Rs256 {
        n: Vec<u8>,
        e: Vec<u8>,
    },
}

impl PublicKey {
    /// Reads a COSE key: the type (1), the algorithm (3), and the
    /// parameters of the type: the curve (-1) and x and y (-2, -3) of an
    /// EC2 or an OKP key, and n and e (-1, -2) of an RSA key.
    pub fn from_cose(bytes: &[u8]) -> Result<PublicKey, Refused> {
        let cose = read_cbor(bytes, "COSE key")?;
        let Some(map) = cose.as_map() else {
            return refused("the COSE key of the reply is not a map");
        };
        let int = |label: i64| key_of(map, label).and_then(integer);
        let bytes = |label: i64| key_of(map, label).and_then(Value::as_bytes);
        let (kty, alg, crv) = (int(1), int(3), int(-1));
        match (kty, alg) {
            (Some(2), Some(ES256)) => {
                let (Some(x), Some(y)) = (bytes(-2), bytes(-3)) else {
                    return refused("the EC2 key of the reply has no x or y");
                };
                if crv != Some(1) || x.len() != 32 || y.len() != 32 {
                    return refused("the EC2 key of the reply is not on the curve P-256");
                }
                let point = [&[0x04][..], x, y].concat();
                p256::ecdsa::VerifyingKey::from_sec1_bytes(&point)
                    .map(PublicKey::Es256)
                    .map_err(|_| Refused("the EC2 key of the reply is no point of P-256".into()))
            }
            (Some(1), Some(EDDSA)) => match bytes(-2) {
                Some(x) if crv == Some(6) && x.len() == 32 => Ok(PublicKey::EdDsa(x.clone())),
                _ => refused("the OKP key of the reply is not an Ed25519 key"),
            },
            (Some(3), Some(RS256)) => match (bytes(-1), bytes(-2)) {
                (Some(n), Some(e)) if n.len() >= 256 && !e.is_empty() => Ok(PublicKey::Rs256 {
                    n: n.clone(),
                    e: e.clone(),
                }),
                _ => refused("the RSA key of the reply has no modulus of 2048 bits or more"),
            },
            _ => refused(format!(
                "the key of the reply has the algorithm {}: riff takes ES256, EdDSA and RS256",
                alg.map_or_else(|| "of no number".to_owned(), |alg| alg.to_string())
            )),
        }
    }

    /// The COSE number of the algorithm of the key.
    pub fn alg(&self) -> i64 {
        match self {
            PublicKey::Es256(_) => ES256,
            PublicKey::EdDsa(_) => EDDSA,
            PublicKey::Rs256 { .. } => RS256,
        }
    }

    /// True when `signature` is a signature of the key over `message`.
    /// An ES256 signature is in DER, as WebAuthn gives it.
    pub fn verify(&self, message: &[u8], signature: &[u8]) -> bool {
        use jsonwebtoken::{Algorithm, DecodingKey};
        let jws = |key: DecodingKey, alg| {
            jsonwebtoken::crypto::verify(&B64.encode(signature), message, &key, alg)
                .unwrap_or(false)
        };
        match self {
            PublicKey::Es256(key) => p256::ecdsa::Signature::from_der(signature)
                .is_ok_and(|signature| key.verify(message, &signature).is_ok()),
            PublicKey::EdDsa(x) => match DecodingKey::from_ed_components(&B64.encode(x)) {
                Ok(key) => jws(key, Algorithm::EdDSA),
                Err(_) => false,
            },
            PublicKey::Rs256 { n, e } => {
                jws(DecodingKey::from_rsa_raw_components(n, e), Algorithm::RS256)
            }
        }
    }
}

/// Reads one CBOR value from all of `bytes`.
fn read_cbor(bytes: &[u8], what: &str) -> Result<Value, Refused> {
    let mut reader = bytes;
    let value = ciborium::de::from_reader_with_recursion_limit::<Value, _>(&mut reader, DEPTH)
        .map_err(|_| Refused(format!("the {what} of the reply is no CBOR")))?;
    if !reader.is_empty() {
        return refused(format!("the {what} of the reply has bytes after its end"));
    }
    Ok(value)
}

/// The value of the text key `name` of the map `value`.
fn field<'a>(value: &'a Value, name: &str) -> Option<&'a Value> {
    field_of(value.as_map()?, name)
}

fn field_of<'a>(map: &'a [(Value, Value)], name: &str) -> Option<&'a Value> {
    map.iter()
        .find(|(key, _)| key.as_text() == Some(name))
        .map(|(_, value)| value)
}

/// The value of the integer key `label` of a COSE map.
fn key_of(map: &[(Value, Value)], label: i64) -> Option<&Value> {
    map.iter()
        .find(|(key, _)| integer(key) == Some(label))
        .map(|(_, value)| value)
}

fn integer(value: &Value) -> Option<i64> {
    value
        .as_integer()
        .and_then(|int| i64::try_from(i128::from(int)).ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use riff_core::device::fake::{self, Authenticator, Cbor, Proof, Shape};

    const RSA_KEY: &str = include_str!("../testdata/test-only-rsa-key.pem");
    const JWKS: &str = include_str!("../testdata/test-only-jwks.json");

    /// A test-only Ed25519 key: its PKCS#8 form, and its public key.
    const ED_PKCS8: &str = "302e020100300506032b657004220420\
        192189ae5d0553fbd49322f0821c454eb416c10808fa5a2035b9d3040f2f58c1";
    const ED_PUBLIC: &str = "5159842187f24f5d24b7d17a0ea15b01ca92d646682dfb19658bf811a9006198";

    fn hex(text: &str) -> Vec<u8> {
        (0..text.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap())
            .collect()
    }

    fn party() -> Party {
        Party::of("https://riff.example.com")
    }

    fn reply(shape: Shape) -> (Authenticator, KeyReply) {
        let key = Authenticator::new();
        let options = options(&party(), "mike", b"handle", b"challenge", &[], "yubikey");
        let reply = key.reply_as("T", &options, &party().origin, shape);
        (key, reply)
    }

    fn reason(shape: Shape) -> String {
        check(&party(), b"challenge", &reply(shape).1)
            .unwrap_err()
            .0
    }

    #[test]
    fn a_packed_reply_and_a_reply_with_no_attestation_give_the_key() {
        for shape in [Shape::Packed, Shape::None] {
            let (key, reply) = reply(shape);
            let checked = check(&party(), b"challenge", &reply).unwrap();
            assert_eq!(B64.encode(&checked.credential), key.credential());
            assert_eq!(B64.encode(&checked.public_key), key.public_key());
            assert_eq!(checked.alg, ES256);
        }
    }

    #[test]
    fn each_bad_reply_is_refused_with_its_reason() {
        for (shape, part) in [
            (Shape::BadSignature, "signature of the reply does not check"),
            (
                Shape::OtherChallenge,
                "a challenge that riff-server did not give",
            ),
            (Shape::OtherOrigin, "comes from https://evil.example.com"),
            (Shape::OtherParty, "another relying party"),
            (Shape::NoPresence, "no user presence"),
            (Shape::OtherType, "not webauthn.create"),
            (Shape::OtherRawId, "the rawId of the reply"),
            (Shape::Form("tpm"), "the attestation tpm"),
        ] {
            let reason = reason(shape);
            assert!(reason.contains(part), "{shape:?}: {reason}");
        }
    }

    #[test]
    fn the_challenge_of_another_add_is_refused() {
        let (_, reply) = reply(Shape::Packed);
        let refused = check(&party(), b"another", &reply).unwrap_err();
        assert!(refused.0.contains("did not give"), "{refused}");
    }

    #[test]
    fn a_reply_that_is_not_base64url_or_cbor_is_refused() {
        let (_, good) = reply(Shape::Packed);
        let not_base64 = KeyReply {
            attestation: "*".into(),
            ..good.clone()
        };
        let reason = check(&party(), b"challenge", &not_base64).unwrap_err().0;
        assert!(reason.contains("not base64url"), "{reason}");
        let not_cbor = KeyReply {
            attestation: B64.encode([0xff, 0x00]),
            ..good
        };
        let reason = check(&party(), b"challenge", &not_cbor).unwrap_err().0;
        assert!(reason.contains("no CBOR"), "{reason}");
    }

    #[test]
    fn a_packed_attestation_with_a_certificate_is_refused() {
        let (_, good) = reply(Shape::Packed);
        let object = B64.decode(&good.attestation).unwrap();
        let value = read_cbor(&object, "x").unwrap();
        let data = field(&value, "authData")
            .unwrap()
            .as_bytes()
            .unwrap()
            .clone();
        let with_cert = Cbor::Map(vec![
            (Cbor::text("fmt"), Cbor::text("packed")),
            (
                Cbor::text("attStmt"),
                Cbor::Map(vec![
                    (Cbor::text("alg"), Cbor::Int(ES256)),
                    (Cbor::text("sig"), Cbor::Bytes(vec![1])),
                    (Cbor::text("x5c"), Cbor::Bytes(vec![2])),
                ]),
            ),
            (Cbor::text("authData"), Cbor::Bytes(data)),
        ]);
        let reply = KeyReply {
            attestation: B64.encode(with_cert.encode()),
            ..good
        };
        let reason = check(&party(), b"challenge", &reply).unwrap_err().0;
        assert!(reason.contains("with a certificate"), "{reason}");
    }

    /// Extensions after the COSE key do not go into the key.
    #[test]
    fn the_key_ends_where_its_cbor_ends() {
        let key = Authenticator::new();
        let cose = B64.decode(key.public_key()).unwrap();
        let mut data = vec![0u8; 32];
        data.push(PRESENT | ATTESTED);
        data.extend_from_slice(&[0; 20]);
        data.extend_from_slice(&[0, 1, 9]);
        data.extend_from_slice(&cose);
        data.extend_from_slice(&Cbor::Map(vec![(Cbor::text("ext"), Cbor::Int(1))]).encode());
        let credential = Credential::read(&data).unwrap();
        assert_eq!(credential.id, [9]);
        assert_eq!(credential.public_key, cose);
    }

    #[test]
    fn an_rs256_key_checks_its_signature() {
        let jwks: serde_json::Value = serde_json::from_str(JWKS).unwrap();
        let n = B64.decode(jwks["keys"][0]["n"].as_str().unwrap()).unwrap();
        let e = B64.decode(jwks["keys"][0]["e"].as_str().unwrap()).unwrap();
        let cose = Cbor::Map(vec![
            (Cbor::Int(1), Cbor::Int(3)),
            (Cbor::Int(3), Cbor::Int(RS256)),
            (Cbor::Int(-1), Cbor::Bytes(n)),
            (Cbor::Int(-2), Cbor::Bytes(e)),
        ])
        .encode();
        let key = PublicKey::from_cose(&cose).unwrap();
        assert_eq!(key.alg(), RS256);
        let signing = jsonwebtoken::EncodingKey::from_rsa_pem(RSA_KEY.as_bytes()).unwrap();
        let signature =
            jsonwebtoken::crypto::sign(b"the data", &signing, jsonwebtoken::Algorithm::RS256)
                .unwrap();
        let signature = B64.decode(signature).unwrap();
        assert!(key.verify(b"the data", &signature));
        assert!(!key.verify(b"other data", &signature));
    }

    #[test]
    fn an_eddsa_key_checks_its_signature() {
        let cose = Cbor::Map(vec![
            (Cbor::Int(1), Cbor::Int(1)),
            (Cbor::Int(3), Cbor::Int(EDDSA)),
            (Cbor::Int(-1), Cbor::Int(6)),
            (Cbor::Int(-2), Cbor::Bytes(hex(ED_PUBLIC))),
        ])
        .encode();
        let key = PublicKey::from_cose(&cose).unwrap();
        assert_eq!(key.alg(), EDDSA);
        let signing = jsonwebtoken::EncodingKey::from_ed_der(&hex(ED_PKCS8));
        let signature =
            jsonwebtoken::crypto::sign(b"the data", &signing, jsonwebtoken::Algorithm::EdDSA)
                .unwrap();
        let signature = B64.decode(signature).unwrap();
        assert!(key.verify(b"the data", &signature));
        assert!(!key.verify(b"other data", &signature));
    }

    #[test]
    fn a_key_of_another_algorithm_or_curve_is_refused() {
        let es384 = Cbor::Map(vec![
            (Cbor::Int(1), Cbor::Int(2)),
            (Cbor::Int(3), Cbor::Int(-35)),
            (Cbor::Int(-1), Cbor::Int(2)),
        ])
        .encode();
        let reason = PublicKey::from_cose(&es384).unwrap_err().0;
        assert!(reason.contains("the algorithm -35"), "{reason}");
        let short_x = Cbor::Map(vec![
            (Cbor::Int(1), Cbor::Int(2)),
            (Cbor::Int(3), Cbor::Int(ES256)),
            (Cbor::Int(-1), Cbor::Int(1)),
            (Cbor::Int(-2), Cbor::Bytes(vec![1; 31])),
            (Cbor::Int(-3), Cbor::Bytes(vec![1; 32])),
        ])
        .encode();
        let reason = PublicKey::from_cose(&short_x).unwrap_err().0;
        assert!(reason.contains("P-256"), "{reason}");
    }

    /// The kept key of a fake authenticator, as the state has it.
    fn kept(key: &Authenticator) -> Vec<(String, PersonKey)> {
        let kept = PersonKey {
            credential: key.credential(),
            public_key: key.public_key(),
            added_ms: 0,
            used_ms: 0,
            counter: 0,
        };
        vec![("yubikey".to_owned(), kept)]
    }

    #[test]
    fn a_good_assertion_gives_the_key_and_the_counter() {
        let key = Authenticator::new();
        let options = fake::proof_options(&party().id, &B64.encode(b"seed"), &key);
        for counter in 1..=2 {
            let reply = key.assert("T", &options, &party().origin);
            let asserted = check_assertion(&party(), b"seed", &reply, &kept(&key)).unwrap();
            assert_eq!(asserted.name, "yubikey");
            assert_eq!(asserted.counter, counter);
        }
        let zero = Authenticator::not_counting();
        let options = fake::proof_options(&party().id, &B64.encode(b"seed"), &zero);
        let reply = zero.assert("T", &options, &party().origin);
        assert_eq!(
            check_assertion(&party(), b"seed", &reply, &kept(&zero))
                .unwrap()
                .counter,
            0
        );
    }

    #[test]
    fn each_bad_assertion_is_refused_with_its_reason() {
        let key = Authenticator::new();
        let options = fake::proof_options(&party().id, &B64.encode(b"seed"), &key);
        for (shape, part) in [
            (
                Proof::BadSignature,
                "the signature of the proof does not check",
            ),
            (
                Proof::OtherChallenge,
                "a challenge that riff-server did not give",
            ),
            (Proof::OtherOrigin, "comes from https://evil.example.com"),
            (Proof::OtherParty, "another relying party"),
            (Proof::NoPresence, "no user presence"),
            (Proof::OtherType, "not webauthn.get"),
        ] {
            let reply = key.assert_as("T", &options, &party().origin, shape);
            let reason = check_assertion(&party(), b"seed", &reply, &kept(&key))
                .unwrap_err()
                .0;
            assert!(reason.contains(part), "{shape:?}: {reason}");
        }
        // The challenge of another proof.
        let reply = key.assert("T", &options, &party().origin);
        let reason = check_assertion(&party(), b"other", &reply, &kept(&key))
            .unwrap_err()
            .0;
        assert!(reason.contains("did not give"), "{reason}");
        // A key that is not one of the person.
        let other = Authenticator::new();
        let reason = check_assertion(&party(), b"seed", &reply, &kept(&other))
            .unwrap_err()
            .0;
        assert!(reason.contains("not one of yours"), "{reason}");
        // Short data, and a value that is not base64url.
        let short = ProofReply {
            authenticator_data: B64.encode([0u8; 36]),
            ..reply.clone()
        };
        let reason = check_assertion(&party(), b"seed", &short, &kept(&key))
            .unwrap_err()
            .0;
        assert!(reason.contains("too short"), "{reason}");
        let bad = ProofReply {
            signature: "*".into(),
            ..reply
        };
        let reason = check_assertion(&party(), b"seed", &bad, &kept(&key))
            .unwrap_err()
            .0;
        assert!(reason.contains("not base64url"), "{reason}");
    }

    #[test]
    fn the_options_of_a_proof_name_each_key_and_the_action() {
        let options = proof_options(
            &party(),
            "mike",
            b"seed",
            &["Y3JlZA".into()],
            "pause the whole riff",
        );
        assert_eq!(options.rp_id, "riff.example.com");
        assert_eq!(options.challenge, B64.encode(b"seed"));
        assert_eq!(options.allow_credentials[0].id, "Y3JlZA");
        assert_eq!(options.action, "pause the whole riff");
        assert_eq!(options.timeout, TIMEOUT_MS);
    }

    #[test]
    fn the_party_of_a_url_with_no_port_has_no_port() {
        assert_eq!(
            Party::of("https://riff.example.com").origin,
            "https://riff.example.com"
        );
        assert_eq!(
            Party::of("http://first:7878"),
            Party {
                id: "first".into(),
                origin: "http://first:7878".into()
            }
        );
    }
}
