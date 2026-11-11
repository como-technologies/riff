//! A proof of presence in `riff` (01M75WKC8RCPHQW3NADHRA31SG, 01M75WKSFX84BHHGFX8VJ37DC6, 01M75WKSNREGG95CK40W2A5JPQ):
//! the page of the proof in the browser, the second try of an action
//! with its proof, and the check of riff-server before a step that it
//! does not do itself.
//!
//! ```mermaid
//! sequenceDiagram
//!     participant P as the person
//!     participant R as riff
//!     participant S as riff-server
//!     participant B as browser
//!     R->>S: the action
//!     S-->>R: 403 needs_proof
//!     R->>S: the start of a proof of the action
//!     S-->>R: the URL of the page of the proof
//!     R->>B: open the page
//!     P->>B: press the button, then touch the authenticator
//!     B->>S: the signature
//!     R->>S: where is the proof? (each second)
//!     S-->>R: proved
//!     R->>S: the action again, with the ticket of the proof
//! ```
//!
//! - [`act`] runs an action. When riff-server refuses it with
//!   `needs_proof`, it gives a proof ([`prove`]) and runs the action
//!   again with the ticket of the proof ([`Api::with_proof`]). With the
//!   policy `off`, riff-server refuses nothing, so riff asks for no
//!   proof.
//! - [`prove`] starts a proof of an action, shows the URL of its page,
//!   opens it, and waits for its end, as `riff device add` does.
//! - [`gate`] asks riff-server before a step that it does not do itself:
//!   a step of the daemon of this host, or a step on GitHub. It stops on
//!   a refusal of riff-server. When riff cannot reach riff-server, it
//!   warns and goes on: the daemon of this host has no check of its own.
//! - A session never gives a proof: riff-server refuses the start, and
//!   [`act`] gives the refusal in an agent session and in a container.

use std::future::Future;

use anyhow::{Result, bail};
use riff_client::api::{Api, Refusal};
use riff_core::name::SessionUri;
use riff_core::proof::{Action, NEEDS_PROOF, Policy};
use riff_core::wire::ProofState;

use crate::device::Wait;
use crate::text;

/// How long the check of a step of this host waits for riff-server
/// ([`gate`]).
pub const GATE_WAIT: std::time::Duration = std::time::Duration::from_secs(5);

/// How `riff` asks for a proof: where it shows the lines with the URL
/// of the page, how it opens the page, and how long it waits.
pub struct Asker {
    pub show: Box<dyn Fn(&str) + Send + Sync>,
    pub open: Box<dyn Fn(&str) + Send + Sync>,
    pub wait: Wait,
    /// True in an agent session and in a container: no proof comes from
    /// there, so riff asks for none.
    pub session: bool,
    /// True for a screen with one line for it, as the note line of the
    /// TUI: the URL of the page comes on one short line.
    pub short: bool,
}

impl Asker {
    /// The asker of a terminal: the lines go to stderr, and the page
    /// opens in the browser.
    pub fn terminal() -> Asker {
        Asker {
            show: Box::new(|lines| eprintln!("{lines}")),
            open: Box::new(|url| {
                let _ = open::that_detached(url);
            }),
            wait: Wait::default(),
            session: crate::cycle::in_agent() || crate::cycle::in_container(),
            short: false,
        }
    }

    /// The asker of the TUI: each line goes to `notes`, the note line of
    /// the TUI, and the page opens in the browser.
    pub fn notes(notes: tokio::sync::mpsc::UnboundedSender<String>) -> Asker {
        Asker {
            show: Box::new(move |line| {
                let _ = notes.send(line.to_owned());
            }),
            short: true,
            ..Asker::terminal()
        }
    }

    /// An asker that shows nothing and opens nothing: for a step that
    /// asks for no proof, as the pause of a repository.
    pub fn quiet() -> Asker {
        Asker {
            show: Box::new(|_| {}),
            open: Box::new(|_| {}),
            ..Asker::terminal()
        }
    }
}

/// True when `error` is the refusal `needs_proof` of riff-server.
///
/// ```
/// use reqwest::StatusCode;
/// use riff_client::api::Refusal;
///
/// let refusal = |code: &str| anyhow::Error::from(Refusal {
///     op: "pause".into(),
///     status: StatusCode::FORBIDDEN,
///     code: Some(code.into()),
///     text: "pause the whole riff needs a proof of presence".into(),
/// });
/// assert!(riff::proof::needs_proof(&refusal("needs_proof")));
/// assert!(!riff::proof::needs_proof(&refusal("not_allowed")));
/// assert!(!riff::proof::needs_proof(&anyhow::anyhow!("no route to host")));
/// ```
pub fn needs_proof(error: &anyhow::Error) -> bool {
    error
        .downcast_ref::<Refusal>()
        .is_some_and(|refusal| refusal.code.as_deref() == Some(NEEDS_PROOF))
}

/// A proof that riff-server checked: its ticket, and the name of the key
/// that signed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Proved {
    pub ticket: String,
    pub name: String,
}

/// Gives a proof of `action` with the sign-in of `api`: the start, the
/// page in the browser, and the wait for its end. Gives the proof, or why
/// there is none.
pub async fn prove(api: &Api, action: &Action, asker: &Asker) -> Result<Proved> {
    let text = action.text();
    let started = api.proof_start(action).await?;
    match asker.short {
        true => (asker.show)(&text::proof_note(&text, &started.url)),
        false => (asker.show)(&text::proof_opens(&text, &started.url)),
    }
    (asker.open)(&started.url);
    let span = tokio::time::Instant::now();
    loop {
        match api.proof_wait(&started.ticket).await? {
            ProofState::Proved { name } => {
                return Ok(Proved {
                    ticket: started.ticket,
                    name,
                });
            }
            ProofState::Failed { reason } => bail!(text::proof_failed(&text, &reason)),
            ProofState::Waiting if span.elapsed() >= asker.wait.most => {
                bail!(text::proof_no_reply(&text))
            }
            ProofState::Waiting => tokio::time::sleep(asker.wait.every).await,
        }
    }
}

/// Runs `run` with `api`. When riff-server refuses it with
/// `needs_proof`, gives a proof of `action` and runs it again with the
/// ticket of the proof. In an agent session and in a container, it gives
/// the refusal: a session never gives a proof.
pub async fn act<T, F, Fut>(api: &Api, action: &Action, asker: &Asker, run: F) -> Result<T>
where
    F: Fn(Api) -> Fut,
    Fut: Future<Output = Result<T>>,
{
    match run(api.clone()).await {
        Err(error) if needs_proof(&error) && !asker.session => {
            let proved = prove(api, action, asker).await?;
            run(api.with_proof(&proved.ticket)).await
        }
        done => done,
    }
}

/// The check of riff-server before the step `action` that it does not
/// do itself (01M75WKSNREGG95CK40W2A5JPQ), with a proof when it needs one. It stops on a
/// refusal of riff-server. When riff cannot reach riff-server, it shows
/// a warning with `asker`, and the step goes on.
pub async fn gate(api: &Api, action: &Action, asker: &Asker) -> Result<()> {
    // One short try: a step of this host does not wait for a riff-server
    // that does not answer.
    let api = &api.one_try(GATE_WAIT);
    let used = act(api, action, asker, |api| {
        let action = action.clone();
        async move { api.use_proof(&action).await }
    })
    .await;
    match used {
        Ok(_) => Ok(()),
        Err(error) if is_refusal(&error) => Err(error),
        Err(error) => {
            (asker.show)(&text::proof_unchecked(
                &action.text(),
                &format!("{error:#}"),
            ));
            Ok(())
        }
    }
}

/// True when riff-server refused the call with the code of a refusal:
/// the step stops. A call with no code, for example with no sign-in, is
/// no refusal of the step.
fn is_refusal(error: &anyhow::Error) -> bool {
    error
        .downcast_ref::<Refusal>()
        .is_some_and(|refusal| refusal.code.is_some())
}

/// The refusal of `action` in an agent session and in a container when
/// the policy of the riff is `required` (01M75WKSJTB28ZQV8ADPBQ86WE): for
/// a step that waits a long time before its call, so that a session
/// learns at once that riff-server will refuse it. When riff cannot read
/// the policy, it refuses nothing: riff-server refuses the call then.
pub async fn refuse_in_session(
    api: &Api,
    me: &SessionUri,
    action: &Action,
    asker: &Asker,
) -> Result<()> {
    if !asker.session {
        return Ok(());
    }
    let required = api
        .pauses(me)
        .await
        .is_ok_and(|pauses| pauses.proof == Policy::Required);
    if required {
        bail!(text::proof_in_session(&action.text()));
    }
    Ok(())
}
