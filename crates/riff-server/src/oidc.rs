//! Sign-in with an OpenID Connect provider (R14, R16).
//!
//! # Flow
//!
//! ```mermaid
//! sequenceDiagram
//!     participant R as riff login
//!     participant P as Provider (Google)
//!     participant S as riff-server
//!     R->>S: GET /v1/sign-in
//!     S-->>R: issuer, client ID
//!     R->>P: browser: authorize (PKCE S256)
//!     P-->>R: code, on a loopback port
//!     R->>P: code + verifier
//!     P-->>R: ID token
//!     R->>S: POST /v1/token (token exchange)
//!     S->>P: discovery, JWKS
//!     S-->>R: riff tokens + user
//! ```
//!
//! `riff-server` never sees the code. It checks only the ID token:
//!
//! - The signature matches a key in the JWKS of the provider.
//! - `iss` is the issuer, `aud` is the client ID, and `exp` is in the
//!   future.
//! - `email_verified` is true.
//!
//! The user part of the session URI is the part of the email before
//! the `@`, in lower case (see [`user_of`]). The server fetches the
//! discovery document and the JWKS at each sign-in. A person signs in
//! about once a month, so there is no cache.
//!
//! # Example
//!
//! ```
//! use riff_server::oidc::user_of;
//!
//! assert_eq!(user_of("Mike.Sandman@comotechnologies.io").unwrap(), "mike.sandman");
//! assert!(user_of("no-at-sign").is_err());
//! ```

use std::fmt;

use jsonwebtoken::jwk::JwkSet;
use jsonwebtoken::{DecodingKey, Validation, decode, decode_header};
use riff_core::name::{check, sanitize};
use riff_core::wire::{Discovery, SignInConfig};
use serde::Deserialize;

/// The OpenID Connect provider of one `riff-server`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Provider {
    pub issuer: String,
    pub client_id: String,
    pub client_secret: Option<String>,
}

/// Who signed in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Identity {
    pub email: String,
    /// The user part of the session URI.
    pub user: String,
}

/// Why a sign-in failed.
#[derive(Debug, PartialEq, Eq)]
pub enum SignInError {
    /// The server cannot reach the provider, or its reply is bad.
    Provider(String),
    /// The ID token is not valid.
    Invalid(String),
}

impl fmt::Display for SignInError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SignInError::Provider(why) => write!(f, "the sign-in provider failed: {why}"),
            SignInError::Invalid(why) => write!(f, "the ID token is not valid: {why}"),
        }
    }
}

impl std::error::Error for SignInError {}

#[derive(Deserialize)]
struct Claims {
    email: Option<String>,
    email_verified: Option<bool>,
}

impl Provider {
    /// What `GET /v1/sign-in` gives to `riff login`.
    pub fn config(&self) -> SignInConfig {
        SignInConfig {
            issuer: self.issuer.clone(),
            client_id: self.client_id.clone(),
            client_secret: self.client_secret.clone(),
        }
    }

    /// Fetches the keys of the provider and checks `id_token` with them.
    pub async fn sign_in(
        &self,
        http: &reqwest::Client,
        id_token: &str,
    ) -> Result<Identity, SignInError> {
        let discovery: Discovery = fetch(http, &Discovery::url(&self.issuer)).await?;
        let jwks: JwkSet = fetch(http, &discovery.jwks_uri).await?;
        self.verify(&jwks, id_token)
    }

    /// Checks `id_token` against `jwks`. See the module docs for the
    /// rules.
    pub fn verify(&self, jwks: &JwkSet, id_token: &str) -> Result<Identity, SignInError> {
        let invalid = |e: jsonwebtoken::errors::Error| SignInError::Invalid(e.to_string());
        let header = decode_header(id_token).map_err(invalid)?;
        let kid = header
            .kid
            .ok_or_else(|| SignInError::Invalid("the token has no key ID".into()))?;
        let jwk = jwks
            .find(&kid)
            .ok_or_else(|| SignInError::Invalid(format!("the provider has no key {kid}")))?;
        let key = DecodingKey::from_jwk(jwk).map_err(invalid)?;
        let mut validation = Validation::new(header.alg);
        validation.set_audience(&[&self.client_id]);
        validation.set_issuer(&issuers(&self.issuer));
        validation.set_required_spec_claims(&["exp", "iss", "aud"]);
        let claims = decode::<Claims>(id_token, &key, &validation)
            .map_err(invalid)?
            .claims;
        if claims.email_verified != Some(true) {
            return Err(SignInError::Invalid("the email is not verified".into()));
        }
        let email = claims
            .email
            .ok_or_else(|| SignInError::Invalid("the token has no email".into()))?;
        let user = user_of(&email)?;
        Ok(Identity { email, user })
    }
}

/// The user part of the session URI for `email`: the part before the
/// `@`, in lower case, with each character that a URI part cannot hold
/// replaced by `-`.
pub fn user_of(email: &str) -> Result<String, SignInError> {
    let local = email
        .rsplit_once('@')
        .map(|(local, _)| local)
        .ok_or_else(|| SignInError::Invalid(format!("not an email: {email}")))?;
    let user = sanitize(&local.to_lowercase());
    check("user", &user).map_err(|e| SignInError::Invalid(e.to_string()))?;
    Ok(user)
}

/// The `iss` values to accept. Google also issues tokens with its
/// issuer host alone, without `https://`.
fn issuers(issuer: &str) -> Vec<String> {
    let issuer = issuer.trim_end_matches('/');
    let mut all = vec![issuer.to_owned()];
    if let Some(host) = issuer.strip_prefix("https://") {
        all.push(host.to_owned());
    }
    all
}

async fn fetch<T: serde::de::DeserializeOwned>(
    http: &reqwest::Client,
    url: &str,
) -> Result<T, SignInError> {
    let provider = |e: reqwest::Error| SignInError::Provider(format!("{url}: {e}"));
    http.get(url)
        .send()
        .await
        .map_err(provider)?
        .error_for_status()
        .map_err(provider)?
        .json()
        .await
        .map_err(provider)
}

#[cfg(test)]
mod tests {
    use super::*;
    use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
    use serde_json::{Value, json};

    const KEY: &str = include_str!("../testdata/test-only-rsa-key.pem");
    const JWKS: &str = include_str!("../testdata/test-only-jwks.json");
    const ISSUER: &str = "https://issuer.test";

    fn provider() -> Provider {
        Provider {
            issuer: ISSUER.into(),
            client_id: "riff-client".into(),
            client_secret: None,
        }
    }

    fn jwks() -> JwkSet {
        serde_json::from_str(JWKS).unwrap()
    }

    fn now() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
    }

    fn claims() -> Value {
        json!({
            "iss": ISSUER,
            "aud": "riff-client",
            "exp": now() + 3600,
            "email": "Mike@comotechnologies.io",
            "email_verified": true,
        })
    }

    fn sign(claims: &Value, kid: &str) -> String {
        let mut header = Header::new(Algorithm::RS256);
        header.kid = Some(kid.into());
        encode(
            &header,
            claims,
            &EncodingKey::from_rsa_pem(KEY.as_bytes()).unwrap(),
        )
        .unwrap()
    }

    fn with(key: &str, value: Value) -> Value {
        let mut claims = claims();
        claims[key] = value;
        claims
    }

    fn refused(claims: &Value) -> bool {
        matches!(
            provider().verify(&jwks(), &sign(claims, "test")),
            Err(SignInError::Invalid(_))
        )
    }

    #[test]
    fn a_valid_token_gives_the_user() {
        let identity = provider().verify(&jwks(), &sign(&claims(), "test"));
        assert_eq!(
            identity,
            Ok(Identity {
                email: "Mike@comotechnologies.io".into(),
                user: "mike".into(),
            })
        );
    }

    #[test]
    fn the_host_alone_is_a_valid_issuer() {
        let claims = with("iss", json!("issuer.test"));
        assert!(provider().verify(&jwks(), &sign(&claims, "test")).is_ok());
    }

    #[test]
    fn bad_claims_are_refused() {
        assert!(refused(&with("iss", json!("https://other.test"))));
        assert!(refused(&with("aud", json!("other-client"))));
        assert!(refused(&with("exp", json!(now() - 3600))));
        assert!(refused(&with("email_verified", json!(false))));
        let mut no_email = claims();
        no_email.as_object_mut().unwrap().remove("email");
        assert!(refused(&no_email));
        let mut no_exp = claims();
        no_exp.as_object_mut().unwrap().remove("exp");
        assert!(refused(&no_exp));
    }

    #[test]
    fn unknown_keys_and_bad_signatures_are_refused() {
        let token = sign(&claims(), "other");
        assert!(provider().verify(&jwks(), &token).is_err());
        let token = sign(&claims(), "test");
        let (head, _) = token.rsplit_once('.').unwrap();
        let forged = format!("{head}.AAAA");
        assert!(provider().verify(&jwks(), &forged).is_err());
        assert!(provider().verify(&jwks(), "not a token").is_err());
    }

    #[test]
    fn users_come_from_the_email() {
        assert_eq!(user_of("a+b@x.io").unwrap(), "a-b");
        assert!(user_of("@x.io").is_err());
    }
}
