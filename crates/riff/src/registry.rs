//! The anonymous pull of a session image (01M5SMRRAG1WPBJMQKKTPSRHRP).
//!
//! # Design
//!
//! riff manages no registry login. The daemon runs `podman pull` as the
//! person on the host, so only a `podman login` of that person counts.
//! A pull request that pins an image that a host cannot pull merges with
//! a green full check, and then each new session of the repository stops
//! on that host. So the verify of such a pull request asks the registry
//! the question that podman asks with no login, and `riff image --check`
//! lets a person ask it too (01M5SMSB0NFBD1XHEW3BSE4BDP).
//!
//! [`anonymous`] needs no podman. It uses the registry API of the OCI
//! distribution spec:
//!
//! ```mermaid
//! sequenceDiagram
//!     participant R as riff
//!     participant G as registry, for example ghcr.io
//!     R->>G: GET /v2/PATH/manifests/DIGEST
//!     G-->>R: 401 and WWW-Authenticate: Bearer realm, service, scope
//!     R->>G: GET realm?service=…&scope=repository:PATH:pull, with no login
//!     G-->>R: 200 and an anonymous token
//!     R->>G: GET /v2/PATH/manifests/DIGEST with the token
//!     G-->>R: 200 for a public package, 403 or 404 for the rest
//! ```
//!
//! A registry that gives the manifest at the first request needs no
//! token. A 200 at the last request is [`Pull::Open`]. A registry that
//! answers with another status gives [`Pull::Closed`] with the status. A
//! registry that does not answer, or answers with a 5xx, is an error: the
//! question has no answer yet.
//!
//! A registry on `localhost` or `127.0.0.1` speaks HTTP, as podman allows
//! for it. Each other registry speaks HTTPS, and so does the URL of its
//! token.
//!
//! ```
//! use riff::registry::{Challenge, base_of};
//!
//! assert_eq!(base_of("ghcr.io"), "https://ghcr.io");
//! assert_eq!(base_of("127.0.0.1:5000"), "http://127.0.0.1:5000");
//!
//! let challenge = Challenge::parse(
//!     r#"Bearer realm="https://ghcr.io/token",service="ghcr.io",scope="repository:o/i:pull""#,
//! )
//! .unwrap();
//! assert_eq!(challenge.realm, "https://ghcr.io/token");
//! assert_eq!(challenge.service.as_deref(), Some("ghcr.io"));
//! assert_eq!(challenge.scope.as_deref(), Some("repository:o/i:pull"));
//! // A registry that asks for a login with a user name and a password.
//! assert!(Challenge::parse(r#"Basic realm="registry""#).is_none());
//! ```

use std::time::Duration;

use anyhow::{Context, Result, bail};
use reqwest::header::{ACCEPT, WWW_AUTHENTICATE};
use reqwest::{Client, Response, StatusCode, Url};
use serde::Deserialize;

use crate::session_image::Image;

/// The media types of a manifest, an index, and their Docker forms.
const MANIFESTS: &str = "application/vnd.oci.image.index.v1+json, \
     application/vnd.oci.image.manifest.v1+json, \
     application/vnd.docker.distribution.manifest.list.v2+json, \
     application/vnd.docker.distribution.manifest.v2+json";

/// How long riff waits for the registry, for connect and for each read.
const WAIT: Duration = Duration::from_secs(20);

/// The answer of a registry to a pull with no login.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pull {
    /// The registry gives the manifest of the image with no login.
    Open,
    /// The registry does not give it. The text says what the registry
    /// answered.
    Closed(String),
}

/// The base URL of the registry `host`: HTTP for a registry on this
/// machine, else HTTPS.
pub fn base_of(host: &str) -> String {
    let name = host.split(':').next().unwrap_or(host);
    match name {
        "localhost" | "127.0.0.1" => format!("http://{host}"),
        _ => format!("https://{host}"),
    }
}

/// The `Bearer` challenge in the `WWW-Authenticate` header of a 401.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Challenge {
    /// The URL that gives a token.
    pub realm: String,
    /// The service that the token is for.
    pub service: Option<String>,
    /// The scope that the registry asks for.
    pub scope: Option<String>,
}

impl Challenge {
    /// Reads the header. `None` when it is not a `Bearer` challenge with a
    /// realm.
    pub fn parse(header: &str) -> Option<Challenge> {
        let mut rest = header.trim().strip_prefix("Bearer")?.trim_start();
        let (mut realm, mut service, mut scope) = (None, None, None);
        while !rest.is_empty() {
            let (key, after) = rest.split_once('=')?;
            let value_and_rest = after.strip_prefix('"')?;
            let (value, after) = value_and_rest.split_once('"')?;
            match key.trim() {
                "realm" => realm = Some(value.to_owned()),
                "service" => service = Some(value.to_owned()),
                "scope" => scope = Some(value.to_owned()),
                _ => {}
            }
            rest = after.trim_start_matches([',', ' ']);
        }
        Some(Challenge {
            realm: realm?,
            service,
            scope,
        })
    }
}

/// The token reply of a registry. Docker registries use `token`, OAuth
/// ones `access_token`.
#[derive(Deserialize)]
struct Token {
    token: Option<String>,
    access_token: Option<String>,
}

/// Asks the registry of `image` for its manifest, with no login.
pub async fn anonymous(image: &Image) -> Result<Pull> {
    let client = Client::builder()
        .user_agent("riff")
        .connect_timeout(WAIT)
        .read_timeout(WAIT)
        .build()
        .context("cannot make the HTTP client of the registry")?;
    ask(&client, &base_of(image.host()), image).await
}

/// [`anonymous`] for code that does not run in an async task: it runs
/// the question on a thread of its own.
pub fn anonymous_here(image: &Image) -> Result<Pull> {
    std::thread::scope(|scope| {
        scope
            .spawn(|| {
                tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .context("cannot start the question to the registry")?
                    .block_on(anonymous(image))
            })
            .join()
            .unwrap_or_else(|_| bail!("the question to the registry stopped"))
    })
}

async fn ask(client: &Client, base: &str, image: &Image) -> Result<Pull> {
    let url = format!("{base}/v2/{}/manifests/{}", image.path(), image.digest());
    let first = get(client, &url, None).await?;
    match first.status() {
        StatusCode::OK => return Ok(Pull::Open),
        StatusCode::UNAUTHORIZED => {}
        status => return refused(status),
    }
    let challenge = first
        .headers()
        .get(WWW_AUTHENTICATE)
        .and_then(|value| value.to_str().ok())
        .and_then(Challenge::parse);
    let Some(challenge) = challenge else {
        return Ok(Pull::Closed(
            "the registry asks for a login, and names no anonymous token".into(),
        ));
    };
    let mut realm = Url::parse(&challenge.realm).context("the token URL of the registry")?;
    if realm.scheme() != "https" && !base.starts_with("http:") {
        return Ok(Pull::Closed(
            "the registry names a token URL that is not HTTPS".into(),
        ));
    }
    let scope = challenge
        .scope
        .unwrap_or_else(|| format!("repository:{}:pull", image.path()));
    {
        let mut query = realm.query_pairs_mut();
        if let Some(service) = &challenge.service {
            query.append_pair("service", service);
        }
        query.append_pair("scope", &scope);
    }
    let reply = get(client, realm.as_str(), None).await?;
    if !reply.status().is_success() {
        return refused(reply.status());
    }
    let token: Token = reply
        .json()
        .await
        .context("the registry gave a token that riff cannot read")?;
    let Some(token) = token.token.or(token.access_token) else {
        return Ok(Pull::Closed("the registry gave no anonymous token".into()));
    };
    let second = get(client, &url, Some(&token)).await?;
    if second.status() == StatusCode::OK {
        return Ok(Pull::Open);
    }
    refused(second.status())
}

/// The GET of `url`, with the bearer `token` when there is one.
async fn get(client: &Client, url: &str, token: Option<&str>) -> Result<Response> {
    let mut request = client.get(url).header(ACCEPT, MANIFESTS);
    if let Some(token) = token {
        request = request.bearer_auth(token);
    }
    request
        .send()
        .await
        .with_context(|| format!("cannot ask {url}"))
}

/// The answer for a status that is no success: [`Pull::Closed`], but a
/// 5xx is an error: the registry has not answered the question.
fn refused(status: StatusCode) -> Result<Pull> {
    if status.is_server_error() {
        bail!("the registry answered {status}");
    }
    Ok(Pull::Closed(format!("the registry answered {status}")))
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use axum::Router;
    use axum::extract::{Query, State};
    use axum::http::{HeaderMap, StatusCode, header};
    use axum::response::IntoResponse;
    use axum::routing::get;

    use super::*;

    /// The state of a fake registry: how it answers, and what it saw.
    #[derive(Clone, Default)]
    struct Fake {
        /// The status of a manifest request with the right token.
        manifest: u16,
        /// The status of a token request.
        token: u16,
        /// True when a manifest request with no token gets the manifest.
        open: bool,
        /// The `Authorization` header of each manifest request, and the
        /// query of each token request.
        seen: Arc<Mutex<Vec<String>>>,
    }

    async fn manifest(State(fake): State<Fake>, headers: HeaderMap) -> impl IntoResponse {
        let auth = headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_owned();
        fake.seen.lock().unwrap().push(format!("manifest {auth}"));
        if fake.open || auth == "Bearer anon" {
            let status = if fake.open { 200 } else { fake.manifest };
            return (StatusCode::from_u16(status).unwrap(), HeaderMap::new()).into_response();
        }
        let mut challenge = HeaderMap::new();
        challenge.insert(
            header::WWW_AUTHENTICATE,
            format!(r#"Bearer realm="{}/token",service="reg""#, base(&headers))
                .parse()
                .unwrap(),
        );
        (StatusCode::UNAUTHORIZED, challenge).into_response()
    }

    fn base(headers: &HeaderMap) -> String {
        format!(
            "http://{}",
            headers.get(header::HOST).unwrap().to_str().unwrap()
        )
    }

    async fn token(
        State(fake): State<Fake>,
        Query(query): Query<std::collections::BTreeMap<String, String>>,
    ) -> impl IntoResponse {
        fake.seen.lock().unwrap().push(format!(
            "token {:?} {:?}",
            query.get("service"),
            query.get("scope")
        ));
        let status = StatusCode::from_u16(fake.token).unwrap();
        (status, r#"{"token":"anon"}"#)
    }

    /// Serves `fake`, and gives the image `127.0.0.1:PORT/o/i@sha256:…`.
    async fn serve(fake: Fake) -> Image {
        let app = Router::new()
            .route("/v2/o/i/manifests/{digest}", get(manifest))
            .route("/token", get(token))
            .with_state(fake);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        format!("127.0.0.1:{port}/o/i@sha256:{}", "0".repeat(64))
            .parse()
            .unwrap()
    }

    /// 01M5SMRRAG1WPBJMQKKTPSRHRP: a public package gives the manifest to
    /// the anonymous token. riff sends the scope `repository:PATH:pull`.
    #[tokio::test]
    async fn a_public_package_gives_the_manifest_to_the_anonymous_token() {
        let fake = Fake {
            manifest: 200,
            token: 200,
            ..Fake::default()
        };
        let image = serve(fake.clone()).await;
        assert_eq!(anonymous(&image).await.unwrap(), Pull::Open);
        let seen = fake.seen.lock().unwrap().clone();
        assert_eq!(
            seen,
            [
                "manifest ".to_owned(),
                r#"token Some("reg") Some("repository:o/i:pull")"#.to_owned(),
                "manifest Bearer anon".to_owned(),
            ]
        );
    }

    #[tokio::test]
    async fn a_registry_that_gives_the_manifest_at_once_needs_no_token() {
        let fake = Fake {
            open: true,
            ..Fake::default()
        };
        let image = serve(fake.clone()).await;
        assert_eq!(anonymous(&image).await.unwrap(), Pull::Open);
        assert_eq!(fake.seen.lock().unwrap().len(), 1);
    }

    /// A private package, or a package that is not there: ghcr.io gives
    /// the token, and then 403 or 404 for the manifest.
    #[tokio::test]
    async fn a_package_that_the_anonymous_token_cannot_read_is_closed() {
        for status in [401, 403, 404] {
            let fake = Fake {
                manifest: status,
                token: 200,
                ..Fake::default()
            };
            let image = serve(fake).await;
            let Pull::Closed(why) = anonymous(&image).await.unwrap() else {
                panic!("{status} must close the package");
            };
            assert!(why.starts_with("the registry answered "), "{why}");
            assert!(why.contains(&status.to_string()), "{why}");
        }
    }

    #[tokio::test]
    async fn a_registry_that_gives_no_anonymous_token_is_closed() {
        let fake = Fake {
            token: 401,
            ..Fake::default()
        };
        let image = serve(fake).await;
        let Pull::Closed(why) = anonymous(&image).await.unwrap() else {
            panic!("a refused token must close the package");
        };
        assert!(why.contains("401"), "{why}");
    }

    /// A registry that has trouble, or that no one listens at, has given
    /// no answer to the question.
    #[tokio::test]
    async fn a_registry_with_no_answer_is_an_error() {
        let fake = Fake {
            token: 503,
            ..Fake::default()
        };
        let image = serve(fake).await;
        let error = anonymous(&image).await.unwrap_err().to_string();
        assert!(error.contains("503"), "{error}");

        let gone: Image = format!("127.0.0.1:9/o/i@sha256:{}", "0".repeat(64))
            .parse()
            .unwrap();
        assert!(anonymous(&gone).await.is_err());
    }

    #[test]
    fn a_challenge_with_a_comma_in_its_scope_is_read_whole() {
        let challenge = Challenge::parse(
            r#"Bearer realm="https://r/token", service="r", scope="repository:o/i:pull,push""#,
        )
        .unwrap();
        assert_eq!(challenge.scope.as_deref(), Some("repository:o/i:pull,push"));
        assert!(Challenge::parse("Bearer").is_none());
        assert!(Challenge::parse(r#"Bearer service="r""#).is_none());
    }

    #[test]
    fn the_sync_question_works_in_and_out_of_a_task() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let fake = Fake {
            open: true,
            ..Fake::default()
        };
        let image = runtime.block_on(serve(fake));
        assert_eq!(anonymous_here(&image).unwrap(), Pull::Open);
        let inside = runtime.block_on(async { anonymous_here(&image) });
        assert_eq!(inside.unwrap(), Pull::Open);
    }
}
