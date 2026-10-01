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
//!   `/v1/token` needs a live access token. A request acts only as the
//!   [`SignedIn`] caller of its token: the `me` of the request must
//!   have the same user and session ID, or the reply is 403 (R104). `/v1/revoke` always needs
//!   one. A missing token, or a token or proof that the server refuses,
//!   gives 401 with a `WWW-Authenticate` challenge. The challenge names
//!   the resource metadata.
//! - With sign-in, each post needs a signature from the device key of
//!   its token ([`SignedIn::check_post`], R197), or the reply is 403.
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

use std::collections::{HashSet, VecDeque};

use riff_core::dpop::{ALG, MAX_AGE, MAX_SKEW};
use riff_core::name::Who;
use riff_core::wire::{Post, ResourceMetadata, ServerMetadata, TOKEN_EXCHANGE};

use crate::oidc::Provider;

/// The path of the protected resource metadata (RFC 9728).
pub const RESOURCE_METADATA_PATH: &str = "/.well-known/oauth-protected-resource";

/// The path of the authorization server metadata (RFC 8414).
pub const SERVER_METADATA_PATH: &str = "/.well-known/oauth-authorization-server";

/// The path of the token endpoint.
pub const TOKEN_PATH: &str = "/v1/token";

/// Each grant type that the token endpoint takes. A token exchange
/// swaps an ID token of the provider for a person pair, or a person
/// access token for a session pair.
pub const GRANT_TYPES: &[&str] = &["refresh_token", TOKEN_EXCHANGE];

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
    /// The email of the owner of a new riff
    /// (01M3JN3ASSV9SA0QZKXXJ0RTEV). A riff that has an owner keeps it.
    pub owner: Option<String>,
    /// The OpenID Connect provider that people sign in with. Without
    /// it, nobody can sign in.
    pub provider: Option<Provider>,
    /// The times of the lease, for a server with a store.
    pub lease: crate::lease::Timing,
    /// The times of the owner role (01M3Q5460YESBSQHTV3M15PE53).
    pub owner_role: crate::owner::Timing,
    /// How long the writer tries a chunk of the log.
    pub log: crate::log::Timing,
    /// When the server writes checkpoints, and which it keeps.
    pub checkpoint: crate::checkpoint::Settings,
    /// The least time between two writes of the token store (R127).
    pub save_every: std::time::Duration,
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
            owner: None,
            provider: None,
            lease: crate::lease::Timing::default(),
            owner_role: crate::owner::Timing::default(),
            log: crate::log::Timing::default(),
            checkpoint: crate::checkpoint::Settings::default(),
            save_every: crate::SAVE_EVERY,
        }
    }

    /// True for a riff with no sign-in: no provider, and no
    /// [`Config::require_sign_in`]. It trusts each caller: a person runs
    /// it only on a network that they trust (R203). So its reader counts
    /// each of its messages as verified (R211).
    ///
    /// ```
    /// use riff_server::auth::Config;
    /// use riff_server::oidc::Provider;
    ///
    /// let mut config = Config::new("http://0.0.0.0:7878");
    /// assert!(config.trusted());
    /// config.require_sign_in = true;
    /// assert!(!config.trusted());
    /// config.require_sign_in = false;
    /// config.provider = Some(Provider {
    ///     issuer: "https://accounts.google.com".into(),
    ///     client_id: "riff".into(),
    ///     client_secret: None,
    ///     allowed_domains: vec!["comotechnologies.io".into()],
    /// });
    /// assert!(!config.trusted());
    /// ```
    pub fn trusted(&self) -> bool {
        self.provider.is_none() && !self.require_sign_in
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
            grant_types_supported: GRANT_TYPES.iter().copied().map(Into::into).collect(),
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

/// Who the access token of a request acts as: the user, and the
/// session of a session token. With [`Config::require_sign_in`], the
/// server puts it in the extensions of each request that passed the
/// token check.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignedIn {
    pub who: Who,
    /// The thumbprint of the device key of the token.
    pub jkt: String,
}

impl SignedIn {
    /// Refuses a request whose `me` is another user or another session
    /// than the token (R104).
    ///
    /// ```
    /// use riff_core::name::{SessionUri, Who};
    /// use riff_server::auth::SignedIn;
    ///
    /// let caller = SignedIn { who: Who::new("mike", Some("a6cf")).unwrap(), jkt: "k".into() };
    /// let me: SessionUri = "riff://mike@pangolin/como-technologies/riff?session=a6cf".parse().unwrap();
    /// assert!(caller.may_act_as(me.who()).is_ok());
    /// let other: SessionUri = "riff://mike@pangolin/como-technologies/riff?session=b7d0".parse().unwrap();
    /// assert!(caller.may_act_as(other.who()).is_err());
    /// ```
    pub fn may_act_as(&self, who: &Who) -> Result<(), String> {
        if &self.who == who {
            return Ok(());
        }
        Err(format!(
            "this token acts only as {}, not as {who}",
            self.who
        ))
    }

    /// Checks the signature of a post from this caller (R197,
    /// 01M3T411N0HM699VJXW6RTVWKB). The post needs its payload and a
    /// signature over it from the device key of the token. The payload
    /// must hold the fields of the post, and a signed time at most
    /// [`MAX_AGE`] seconds old and at most [`MAX_SKEW`] seconds in the
    /// future. It never encodes the payload again. `now_ms` is the time
    /// of the server, in milliseconds since the Unix epoch. Gives the
    /// signed time.
    ///
    /// ```
    /// use riff_core::dpop::Key;
    /// use riff_core::name::SessionUri;
    /// use riff_core::wire::Post;
    /// use riff_server::auth::SignedIn;
    ///
    /// let me: SessionUri = "riff://mike@pangolin/como-technologies/riff?session=a6cf".parse()?;
    /// let key = Key::generate();
    /// let caller = SignedIn { who: me.who().clone(), jkt: key.thumbprint() };
    /// let mut post = Post::new(&me, Some("design".parse()?), vec![], "look");
    /// assert!(caller.check_post(&post, 9_000).is_err());
    ///
    /// post.sign(&key, 9_000);
    /// assert_eq!(caller.check_post(&post, 9_000), Ok(9_000));
    ///
    /// // The call says another body than the payload.
    /// let changed = Post { body: "do not look".into(), ..post.clone() };
    /// assert!(caller.check_post(&changed, 9_000).unwrap_err().contains("payload"));
    ///
    /// // A signature from another key than the key of the token.
    /// post.sign(&Key::generate(), 9_000);
    /// assert!(caller.check_post(&post, 9_000).is_err());
    /// # Ok::<(), riff_core::name::NameError>(())
    /// ```
    pub fn check_post(&self, post: &Post, now_ms: u64) -> Result<u64, String> {
        let (Some(sig), Some(payload)) = (&post.sig, &post.payload) else {
            return Err("this server needs a signature on each message. Update riff.".into());
        };
        let (jkt, signed) = riff_core::signed::check(payload, sig)
            .map_err(|e| format!("the message signature is refused: {e}"))?;
        let call = riff_core::signed::Content {
            from: post.me.who(),
            lead: post.me.lead(),
            thread: post.thread.as_ref(),
            to: &post.to,
            body: &post.body,
            kind: post.kind,
            at_ms: signed.at_ms,
        };
        if !signed.covers(&call) {
            return Err("the payload does not hold the fields of the post".into());
        }
        self.may_act_as(&signed.from)?;
        if jkt != self.jkt {
            return Err("the message is signed with another key than the key of the token".into());
        }
        let at_ms = signed.at_ms;
        if now_ms.saturating_sub(at_ms) > MAX_AGE * 1000 {
            return Err("the message time is too old. Check the clock of this machine.".into());
        }
        if at_ms > now_ms.saturating_add(MAX_SKEW * 1000) {
            return Err(
                "the message time is in the future. Check the clock of this machine.".into(),
            );
        }
        Ok(at_ms)
    }
}

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

/// The most proof IDs that [`Replay`] keeps (R114).
pub const MAX_PROOFS: usize = 100_000;

/// The proof IDs that the server saw, so that no proof works twice.
/// It forgets an ID when its proof is too old to pass the time check.
/// It keeps at most [`MAX_PROOFS`] IDs. When it must forget an ID
/// early, it refuses each proof as old as that one, so a forgotten
/// proof still cannot work twice (R114).
#[derive(Default)]
pub struct Replay {
    seen: HashSet<String>,
    /// Each ID with the time when it can go and its `iat`, oldest
    /// first.
    order: VecDeque<(u64, u64, String)>,
    /// A proof with an `iat` below this came before a forgotten ID.
    floor: u64,
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
        while let Some((until, _, _)) = self.order.front()
            && *until < now
        {
            let (_, _, old) = self.order.pop_front().expect("a front entry");
            self.seen.remove(&old);
        }
        if iat < self.floor || self.seen.contains(jti) {
            return false;
        }
        if self.order.len() >= MAX_PROOFS
            && let Some((_, old_iat, old)) = self.order.pop_front()
        {
            self.seen.remove(&old);
            self.floor = self.floor.max(old_iat.saturating_add(1));
        }
        self.seen.insert(jti.to_owned());
        // A proof passes the time check until iat + MAX_AGE, and iat is
        // at most now + MAX_SKEW.
        self.order
            .push_back((now + MAX_SKEW + MAX_AGE, iat, jti.to_owned()));
        true
    }

    /// Refuses each proof issued before `iat`, in seconds since the Unix
    /// epoch. A server calls it when it starts to serve (R142).
    ///
    /// ```
    /// use riff_server::auth::Replay;
    ///
    /// let mut replay = Replay::default();
    /// replay.refuse_before(1_000);
    /// assert!(!replay.first_use("j-1", 999, 1_000));
    /// assert!(replay.first_use("j-2", 1_000, 1_000));
    /// ```
    pub fn refuse_before(&mut self, iat: u64) {
        self.floor = self.floor.max(iat);
    }

    /// The number of proof IDs that it keeps.
    pub fn len(&self) -> usize {
        self.order.len()
    }

    /// True when it keeps no proof ID.
    pub fn is_empty(&self) -> bool {
        self.order.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use riff_core::dpop::Key;
    use riff_core::name::SessionUri;
    use riff_core::selector::Selector;

    use super::*;

    fn signed(key: &Key, at_ms: u64) -> (SignedIn, Post) {
        let me: SessionUri = "riff://mike@pangolin/como-technologies/riff?session=a6cf"
            .parse()
            .unwrap();
        let caller = SignedIn {
            who: me.who().clone(),
            jkt: key.thumbprint(),
        };
        let mut post = Post::new(&me, None, vec![Selector::session("b7d0")], "go");
        post.sign(key, at_ms);
        (caller, post)
    }

    #[test]
    fn a_post_needs_a_time_near_the_server_time() {
        let key = Key::generate();
        let now = 1_000_000;
        let (caller, post) = signed(&key, now - MAX_AGE * 1000);
        assert!(caller.check_post(&post, now).is_ok());
        let (caller, post) = signed(&key, now - MAX_AGE * 1000 - 1);
        let error = caller.check_post(&post, now).unwrap_err();
        assert!(error.contains("too old"), "{error}");
        let (caller, post) = signed(&key, now + MAX_SKEW * 1000);
        assert!(caller.check_post(&post, now).is_ok());
        let (caller, post) = signed(&key, now + MAX_SKEW * 1000 + 1);
        let error = caller.check_post(&post, now).unwrap_err();
        assert!(error.contains("in the future"), "{error}");
    }

    #[test]
    fn a_post_must_be_signed_as_the_caller() {
        let key = Key::generate();
        let (caller, mut post) = signed(&key, 5);
        post.me = "riff://mike@pangolin/como-technologies/riff?session=1ead"
            .parse()
            .unwrap();
        post.sign(&key, 5);
        let error = caller.check_post(&post, 5).unwrap_err();
        assert!(error.contains("acts only as mike/a6cf"), "{error}");
    }

    #[test]
    fn a_changed_post_is_refused() {
        let key = Key::generate();
        let (caller, mut post) = signed(&key, 5);
        post.body = "stop".into();
        let error = caller.check_post(&post, 5).unwrap_err();
        assert!(error.contains("does not hold the fields"), "{error}");
        // A payload of another body under the old signature.
        let (_, other) = signed(&key, 5);
        let mut changed = other.clone();
        changed.payload = Some(
            riff_core::signed::Content {
                body: "stop",
                ..other.content().unwrap()
            }
            .payload(),
        );
        let error = caller.check_post(&changed, 5).unwrap_err();
        assert!(error.contains("not valid for this message"), "{error}");
        post.sig = None;
        let error = caller.check_post(&post, 5).unwrap_err();
        assert!(error.contains("needs a signature"), "{error}");
    }

    #[test]
    fn replay_forgets_old_proofs_in_order() {
        let mut replay = Replay::default();
        assert!(replay.first_use("a", 100, 100));
        assert!(replay.first_use("b", 200, 200));
        assert!(replay.first_use("c", 470, 470));
        // "a" is too old for the time check now, so the store forgot it.
        assert_eq!(replay.len(), 2);
        assert!(!replay.first_use("b", 200, 470));
    }

    #[test]
    fn a_full_replay_store_takes_new_proofs_and_refuses_forgotten_ones() {
        let mut replay = Replay::default();
        for i in 0..MAX_PROOFS {
            assert!(replay.first_use(&format!("flood-{i}"), 1000, 1000));
        }
        assert!(replay.first_use("fresh", 1001, 1001));
        assert_eq!(replay.len(), MAX_PROOFS);
        // flood-0 is forgotten, but a proof as old as it is refused.
        assert!(!replay.first_use("flood-0", 1000, 1001));
        assert!(!replay.first_use("fresh", 1001, 1001));
        assert!(replay.first_use("fresh-2", 1001, 1001));
    }

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
    fn the_token_endpoint_takes_refresh_and_exchange() {
        let config = Config::new("http://h");
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
