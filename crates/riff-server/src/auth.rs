//! OAuth for `riff-server`, after the MCP authorization spec,
//! revision 2026-07-28 (R22).
//!
//! # Roles
//!
//! `riff-server` is both the resource server and the authorization
//! server. Its public URL ([`Config::public_url`]) is the resource and
//! the issuer. A token that it issues is only for itself, so the
//! audience of each token is the public URL.
//!
//! # Flow
//!
//! ```text
//!  riff ── /v1/who, no token ─────────────────▶ 401, WWW-Authenticate:
//!                                                 DPoP algs="ES256", resource_metadata="…"
//!  riff ── GET /.well-known/oauth-protected-resource ─▶ issuer
//!  riff ── GET /.well-known/oauth-authorization-server ─▶ token endpoint
//!  riff ── POST /v1/token (resource=<public URL>) + DPoP proof ─▶ riff tokens
//!  riff ── /v1/who, Authorization: DPoP <token> + DPoP proof ─▶ 200
//! ```
//!
//! # Rules
//!
//! - A token goes only in the `Authorization` header, never in the
//!   query string. The scheme is `DPoP`. Each request also carries a
//!   `DPoP` header with a proof from the device key of the token (R18,
//!   RFC 9449). A bearer token is refused.
//! - The URL in a proof is the public URL and the path of the request.
//!   The server refuses a proof `jti` that it saw before ([`Replay`]).
//! - The token endpoint needs a proof too. A refresh works only with
//!   the key of the sign-in.
//! - With [`Config::require_sign_in`], each `/v1` route except
//!   `/v1/token` needs a live access token. `/v1/revoke` always needs
//!   one. A missing token, or a token or proof that the server refuses,
//!   gives 401 with a `WWW-Authenticate` challenge. The challenge names
//!   the resource metadata.
//! - The token endpoint refuses a `resource` other than the public URL
//!   with `invalid_target` (RFC 8707).
//! - The server has no authorization endpoint (R83). So the metadata
//!   lists no response type.
//!
//! # Example
//!
//! ```
//! use riff_server::auth::Config;
//!
//! let config = Config::new("https://riff.example.com/");
//! assert_eq!(config.public_url, "https://riff.example.com");
//! assert_eq!(
//!     config.challenge(None),
//!     r#"DPoP algs="ES256", resource_metadata="https://riff.example.com/.well-known/oauth-protected-resource""#
//! );
//! assert!(config.is_resource("https://RIFF.example.com/"));
//! ```

use std::collections::HashMap;

use riff_core::dpop::{ALG, MAX_AGE, MAX_SKEW};
use riff_core::wire::{ResourceMetadata, ServerMetadata, TOKEN_EXCHANGE};

use crate::oidc::Provider;

/// The path of the protected resource metadata (RFC 9728).
pub const RESOURCE_METADATA_PATH: &str = "/.well-known/oauth-protected-resource";

/// The path of the authorization server metadata (RFC 8414).
pub const SERVER_METADATA_PATH: &str = "/.well-known/oauth-authorization-server";

/// The path of the token endpoint.
pub const TOKEN_PATH: &str = "/v1/token";

/// Each grant type that the token endpoint takes. It takes
/// [`TOKEN_EXCHANGE`] too when the server has a provider.
pub const GRANT_TYPES: &[&str] = &["refresh_token"];

/// The settings of one `riff-server`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    /// The URL where people reach the server, with no trailing slash.
    /// It is the resource and the issuer.
    pub public_url: String,
    /// True: each `/v1` route except `/v1/token` needs an access token.
    pub require_sign_in: bool,
    /// The people who may revoke the tokens of any person (R20).
    pub admins: Vec<String>,
    /// The OpenID Connect provider that people sign in with. Without
    /// it, nobody can sign in.
    pub provider: Option<Provider>,
}

impl Default for Config {
    fn default() -> Self {
        Config::new("http://127.0.0.1:7878")
    }
}

impl Config {
    /// A config with this public URL. Sign-in is not required, and
    /// nobody is an admin.
    pub fn new(public_url: &str) -> Self {
        Config {
            public_url: public_url.trim_end_matches('/').to_owned(),
            require_sign_in: false,
            admins: Vec::new(),
            provider: None,
        }
    }

    /// The protected resource metadata (RFC 9728).
    pub fn resource_metadata(&self) -> ResourceMetadata {
        ResourceMetadata {
            resource: self.public_url.clone(),
            authorization_servers: vec![self.public_url.clone()],
            bearer_methods_supported: vec!["header".into()],
            dpop_signing_alg_values_supported: vec![ALG.into()],
            dpop_bound_access_tokens_required: true,
        }
    }

    /// The authorization server metadata (RFC 8414).
    pub fn server_metadata(&self) -> ServerMetadata {
        ServerMetadata {
            issuer: self.public_url.clone(),
            token_endpoint: format!("{}{TOKEN_PATH}", self.public_url),
            grant_types_supported: GRANT_TYPES
                .iter()
                .copied()
                .chain(self.provider.as_ref().map(|_| TOKEN_EXCHANGE))
                .map(Into::into)
                .collect(),
            response_types_supported: vec![],
            code_challenge_methods_supported: vec!["S256".into()],
            token_endpoint_auth_methods_supported: vec!["none".into()],
            dpop_signing_alg_values_supported: vec![ALG.into()],
        }
    }

    /// The public URL of one path of this server.
    pub fn url(&self, path: &str) -> String {
        format!("{}{path}", self.public_url)
    }

    /// The `WWW-Authenticate` value of a 401 reply. `error` is set when
    /// the server refused a token or a proof.
    pub fn challenge(&self, error: Option<&Refusal>) -> String {
        let metadata = self.url(RESOURCE_METADATA_PATH);
        match error {
            None => format!(r#"DPoP algs="{ALG}", resource_metadata="{metadata}""#),
            Some(Refusal { code, description }) => format!(
                r#"DPoP error="{code}", error_description="{description}", algs="{ALG}", resource_metadata="{metadata}""#
            ),
        }
    }

    /// True when `resource` names this server. The scheme and the host
    /// may be in upper case, and a trailing slash is allowed.
    pub fn is_resource(&self, resource: &str) -> bool {
        resource
            .trim_end_matches('/')
            .eq_ignore_ascii_case(&self.public_url)
    }
}

/// The user of the access token of a request. With
/// [`Config::require_sign_in`], the server puts it in the extensions of
/// each request that passed the token check.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignedIn(pub String);

/// Why the server refused a request that had a token or a proof.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Refusal {
    /// `invalid_token` or `invalid_dpop_proof` (RFC 9449).
    pub code: &'static str,
    pub description: String,
}

impl Refusal {
    pub fn token(description: impl ToString) -> Self {
        Refusal {
            code: "invalid_token",
            description: description.to_string(),
        }
    }

    pub fn proof(description: impl ToString) -> Self {
        Refusal {
            code: "invalid_dpop_proof",
            description: description.to_string(),
        }
    }
}

/// Returns the token of an `Authorization: DPoP <token>` header value.
/// The scheme is not case sensitive. A bearer token gives `None`.
pub fn dpop_token(header: &str) -> Option<&str> {
    let (scheme, token) = header.split_once(' ')?;
    let token = token.trim();
    (scheme.eq_ignore_ascii_case("dpop") && !token.is_empty()).then_some(token)
}

/// The proof IDs that the server saw, so that no proof works twice.
/// It forgets an ID when its proof is too old to pass the time check.
#[derive(Default)]
pub struct Replay {
    seen: HashMap<String, u64>,
}

impl Replay {
    /// True the first time that `jti` comes. `iat` is the time of its
    /// proof and `now` the time now, in seconds since the Unix epoch.
    ///
    /// ```
    /// use riff_server::auth::Replay;
    ///
    /// let mut replay = Replay::default();
    /// assert!(replay.first_use("j-1", 100, 100));
    /// assert!(!replay.first_use("j-1", 100, 101));
    /// ```
    pub fn first_use(&mut self, jti: &str, iat: u64, now: u64) -> bool {
        self.seen.retain(|_, iat| *iat + MAX_AGE + MAX_SKEW >= now);
        self.seen.insert(jti.to_owned(), iat).is_none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_public_url_is_the_resource_and_the_issuer() {
        let config = Config::new("https://riff.example.com");
        let resource = config.resource_metadata();
        let server = config.server_metadata();
        assert_eq!(resource.resource, "https://riff.example.com");
        assert_eq!(resource.authorization_servers, [server.issuer.as_str()]);
        assert_eq!(resource.bearer_methods_supported, ["header"]);
        assert!(resource.dpop_bound_access_tokens_required);
        assert_eq!(server.dpop_signing_alg_values_supported, ["ES256"]);
        assert_eq!(server.token_endpoint, "https://riff.example.com/v1/token");
        assert_eq!(server.code_challenge_methods_supported, ["S256"]);
        assert!(server.response_types_supported.is_empty());
    }

    #[test]
    fn a_provider_adds_the_token_exchange_grant() {
        let mut config = Config::new("http://h");
        assert_eq!(
            config.server_metadata().grant_types_supported,
            ["refresh_token"]
        );
        config.provider = Some(Provider {
            issuer: "https://accounts.google.com".into(),
            client_id: "riff".into(),
            client_secret: None,
            allowed_domains: vec![],
        });
        assert_eq!(
            config.server_metadata().grant_types_supported,
            ["refresh_token", TOKEN_EXCHANGE]
        );
    }

    #[test]
    fn the_resource_must_name_this_server() {
        let config = Config::new("http://127.0.0.1:7878");
        assert!(config.is_resource("http://127.0.0.1:7878"));
        assert!(config.is_resource("HTTP://127.0.0.1:7878/"));
        assert!(!config.is_resource("http://127.0.0.1:7879"));
        assert!(!config.is_resource("https://evil.example.com"));
    }

    #[test]
    fn a_refusal_names_its_code() {
        let config = Config::new("http://h");
        assert_eq!(
            config.challenge(Some(&Refusal::token("the token expired"))),
            r#"DPoP error="invalid_token", error_description="the token expired", algs="ES256", resource_metadata="http://h/.well-known/oauth-protected-resource""#
        );
        assert!(
            config
                .challenge(Some(&Refusal::proof("old")))
                .starts_with(r#"DPoP error="invalid_dpop_proof""#)
        );
    }

    #[test]
    fn dpop_token_reads_only_the_dpop_scheme() {
        assert_eq!(dpop_token("DPoP abc"), Some("abc"));
        assert_eq!(dpop_token("dpop abc"), Some("abc"));
        assert_eq!(dpop_token("Bearer abc"), None);
        assert_eq!(dpop_token("DPoP "), None);
        assert_eq!(dpop_token("DPoP"), None);
    }

    #[test]
    fn replay_forgets_old_proofs() {
        let mut replay = Replay::default();
        assert!(replay.first_use("a", 100, 100));
        assert!(!replay.first_use("a", 100, 100 + MAX_AGE + MAX_SKEW));
        // Too old to pass the time check, so the ID may go.
        assert!(replay.first_use("b", 1_000, 100 + MAX_AGE + MAX_SKEW + 1));
        assert!(replay.first_use("a", 1_000, 1_000));
    }
}
