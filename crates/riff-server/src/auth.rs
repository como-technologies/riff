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
//!                                                 Bearer resource_metadata="…"
//!  riff ── GET /.well-known/oauth-protected-resource ─▶ issuer
//!  riff ── GET /.well-known/oauth-authorization-server ─▶ token endpoint
//!  riff ── POST /v1/token (resource=<public URL>) ─▶ riff tokens
//!  riff ── /v1/who, Authorization: Bearer <token> ─▶ 200
//! ```
//!
//! # Rules
//!
//! - A token goes only in the `Authorization` header, never in the
//!   query string.
//! - With [`Config::require_sign_in`], each `/v1` route except
//!   `/v1/token` needs a live access token. A missing token or a token
//!   that the server refuses gives 401 with a `WWW-Authenticate`
//!   challenge. The challenge names the resource metadata.
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
//!     r#"Bearer resource_metadata="https://riff.example.com/.well-known/oauth-protected-resource""#
//! );
//! assert!(config.is_resource("https://RIFF.example.com/"));
//! ```

use riff_core::wire::{ResourceMetadata, ServerMetadata};

/// The path of the protected resource metadata (RFC 9728).
pub const RESOURCE_METADATA_PATH: &str = "/.well-known/oauth-protected-resource";

/// The path of the authorization server metadata (RFC 8414).
pub const SERVER_METADATA_PATH: &str = "/.well-known/oauth-authorization-server";

/// The path of the token endpoint.
pub const TOKEN_PATH: &str = "/v1/token";

/// Each grant type that the token endpoint takes.
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
        }
    }

    /// The protected resource metadata (RFC 9728).
    pub fn resource_metadata(&self) -> ResourceMetadata {
        ResourceMetadata {
            resource: self.public_url.clone(),
            authorization_servers: vec![self.public_url.clone()],
            bearer_methods_supported: vec!["header".into()],
        }
    }

    /// The authorization server metadata (RFC 8414).
    pub fn server_metadata(&self) -> ServerMetadata {
        ServerMetadata {
            issuer: self.public_url.clone(),
            token_endpoint: format!("{}{TOKEN_PATH}", self.public_url),
            grant_types_supported: GRANT_TYPES.iter().map(|g| (*g).into()).collect(),
            response_types_supported: vec![],
            code_challenge_methods_supported: vec!["S256".into()],
            token_endpoint_auth_methods_supported: vec!["none".into()],
        }
    }

    /// The `WWW-Authenticate` value of a 401 reply. `error` is set when
    /// the request had a token that the server refused.
    pub fn challenge(&self, error: Option<&str>) -> String {
        let metadata = format!("{}{RESOURCE_METADATA_PATH}", self.public_url);
        match error {
            None => format!(r#"Bearer resource_metadata="{metadata}""#),
            Some(error) => format!(
                r#"Bearer error="invalid_token", error_description="{error}", resource_metadata="{metadata}""#
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

/// Returns the token of an `Authorization: Bearer <token>` header value.
/// The scheme is not case sensitive.
pub fn bearer(header: &str) -> Option<&str> {
    let (scheme, token) = header.split_once(' ')?;
    let token = token.trim();
    (scheme.eq_ignore_ascii_case("bearer") && !token.is_empty()).then_some(token)
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
        assert_eq!(server.token_endpoint, "https://riff.example.com/v1/token");
        assert_eq!(server.code_challenge_methods_supported, ["S256"]);
        assert!(server.response_types_supported.is_empty());
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
    fn a_refused_token_gets_invalid_token() {
        let config = Config::new("http://h");
        assert_eq!(
            config.challenge(Some("the token expired")),
            r#"Bearer error="invalid_token", error_description="the token expired", resource_metadata="http://h/.well-known/oauth-protected-resource""#
        );
    }

    #[test]
    fn bearer_reads_the_token() {
        assert_eq!(bearer("Bearer abc"), Some("abc"));
        assert_eq!(bearer("bearer abc"), Some("abc"));
        assert_eq!(bearer("Basic abc"), None);
        assert_eq!(bearer("Bearer "), None);
        assert_eq!(bearer("Bearer"), None);
    }
}
