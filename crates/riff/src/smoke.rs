//! `riff cloud smoke NAME`: the smoke test of a riff in the cloud
//! (01M496JTHN19BZ7YN94993R35X).
//!
//! CI runs it after each deploy of the stage. It signs in as a test
//! account, posts a message, reads it back, and checks that the server
//! runs the build of this `riff`.
//!
//! ```mermaid
//! sequenceDiagram
//!     participant C as riff cloud smoke
//!     participant P as Provider (Google)
//!     participant S as riff-server
//!     C->>S: GET /v1/sign-in
//!     C->>P: refresh token of the test account
//!     P-->>C: ID token
//!     C->>S: token exchange
//!     S-->>C: riff tokens
//!     C->>S: register a session, post a message
//!     C->>S: read the thread
//!     S-->>C: the message, and the build of the server
//!     C->>S: end the session
//! ```
//!
//! The refresh token comes from [`TOKEN_VAR`], never from a file or an
//! argument. Only the stage admits the test account, so the token signs
//! in nowhere else. The sign-in goes to the secret store of `RIFF_HOME`:
//! CI sets `RIFF_HOME` to a new directory.

use anyhow::{Context, Result, bail};
use riff_core::build::Build;
use riff_core::name::{Place, Repo, SessionUri, ThreadName, Who};
use riff_core::wire::Kind;

use crate::api::{self, Api};
use crate::login;

/// The variable that holds the refresh token of the test account.
pub const TOKEN_VAR: &str = "RIFF_SMOKE_TOKEN";

/// The host of the session of the smoke test.
pub const HOST: &str = "smoke";

/// The repository of the session of the smoke test, and so its thread.
pub const REPO: (&str, &str) = ("riff", "smoke");

/// The text of the message of the smoke test with the mark `mark`.
///
/// ```
/// assert_eq!(riff::smoke::body("a1"), "smoke test a1");
/// ```
pub fn body(mark: &str) -> String {
    format!("smoke test {mark}")
}

/// 8 random hex digits: the session ID of one run, and the mark of its
/// message.
fn mark() -> String {
    let mut bytes = [0u8; 4];
    // The OS random source fails only when the OS is broken.
    getrandom::fill(&mut bytes).expect("the OS gives random bytes");
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Runs the smoke test against the server of `api`, signed in with the
/// refresh token `token` of the test account. `step` gets one line for
/// each step that passed. The error names the step that failed.
pub async fn run(api: &Api, token: &str, mut step: impl FnMut(&str)) -> Result<()> {
    let sign_in = login::login_with_refresh_token(api, token)
        .await
        .context("smoke test: sign in")?;
    step(&format!("sign in: {} at {}", sign_in.user, api.base()));

    let mark = mark();
    let me = SessionUri::new(
        Who::new(&sign_in.user, Some(&mark))?,
        Place::new(
            HOST,
            Repo::Git {
                owner: REPO.0.into(),
                name: REPO.1.into(),
            },
            None,
        )?,
    );
    let thread: ThreadName = me
        .place()
        .default_thread()
        .context("the smoke session has a thread")?;
    let session = api
        .clone()
        .signed_in(Some(me.who().session().unwrap_or_default()))?;
    session
        .register(&me)
        .await
        .context("smoke test: register")?;
    let text = body(&mark);
    session
        .post(&me, Some(&thread), &[], &text, Kind::Note)
        .await
        .context("smoke test: post")?;
    step(&format!("post: {text} to {thread}"));

    let read = session
        .read(&me, &thread, true)
        .await
        .context("smoke test: read")?;
    if !read.iter().any(|m| m.message.body == text) {
        bail!("smoke test: read: the thread {thread} has no message {text:?}");
    }
    step(&format!("read: {text}"));

    let this = Build::this();
    match api::server_build() {
        Some(server) if server.matches(&this) => step(&format!("build: {server}")),
        Some(server) => bail!("smoke test: build: the server runs {server}, not {this}"),
        None => bail!("smoke test: build: the server names no build"),
    }
    session.end(&me).await.context("smoke test: end")?;
    step("end: the session ended");
    Ok(())
}
