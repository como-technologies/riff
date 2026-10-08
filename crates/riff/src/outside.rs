//! `riff outside`: run one named command outside the profile of a
//! session, one time, after the owner or an admin approves it (#614).
//!
//! # Design
//!
//! The sandbox of a session is on, with no switch to turn it off. A
//! session that needs one command outside it asks for it:
//!
//! | Who | Runs | Where |
//! |---|---|---|
//! | the session | `riff outside ask --reason REASON -- PROGRAM ARGS` | in its sandbox |
//! | the owner or an admin | `riff outside list` | in a terminal |
//! | the owner or an admin | `riff outside approve ID` or `riff outside deny ID` | in a terminal |
//!
//! 1. `riff outside ask` sends the command, the current folder and the
//!    reason to riff-server (01M4DA9PFR6V3K3FE1568277H3). The server
//!    gives the request an ID, and its post wakes the lead of the person.
//! 2. Then it asks the broker of the session ([`crate::broker`]) for the
//!    operation `outside` with the ID, and waits.
//! 3. An admin approves it with a token of a person
//!    (01M4DA9PJ0MJPBQRTA79CVXEA2). A session has only the grant of its
//!    own session, so no session approves.
//! 4. The broker, outside the sandbox, takes the request from the
//!    server one time (01M4DA9PM89KP332T6BR7V0CDT). It runs the command
//!    and the folder of the server, not of the session, with the stdio
//!    of `riff outside ask`. The exit code of the command is the exit
//!    code of `riff outside ask`.
//!
//! The server posts each step to the thread of the repository: the log
//! of who asked, who approved, the command and the reason
//! (01M4DA9PPFBVPHP57JZQDF4R7R).

use std::ffi::OsString;

use anyhow::{Result, bail};
use riff_core::name::SessionUri;
use riff_core::wire::OutsideRequest;

use crate::api::Api;
use crate::broker::{self, Reply, Request};

/// `riff outside ask`: asks riff-server, then the broker, and gives the
/// exit code of the command.
pub async fn ask(server: &str, reason: &str, program: &OsString, args: &[OsString]) -> Result<i32> {
    let Some(fd) = broker::here() else {
        bail!(crate::text::OUTSIDE_NO_BROKER);
    };
    let me = crate::identity::session(&crate::identity::here(None)?, server)?;
    let api = Api::new(server).signed_in(me.who().session())?;
    let cwd = std::env::current_dir()?;
    let command: Vec<String> = std::iter::once(program)
        .chain(args)
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();
    let asked = api
        .outside_ask(&me, command, &cwd.to_string_lossy(), reason)
        .await?;
    eprintln!("{}", crate::text::outside_asked(&asked.id));
    let request = Request {
        op: "outside".into(),
        args: vec![asked.id.into()],
        cwd,
        env: vec![],
    };
    match tokio::task::spawn_blocking(move || broker::ask(fd, &request)).await?? {
        Reply::Code(code) => Ok(code),
        Reply::Refused(why) => bail!("{}", crate::text::broker_refused(&why)),
    }
}

/// The step of the broker that takes a request from riff-server as the
/// session `me` ([`broker::Take`]). Each call runs on a runtime of its
/// own: the broker serves each request in a thread.
pub fn take(server: &str, me: SessionUri) -> Result<broker::Take> {
    let api = Api::new(server).signed_in(me.who().session())?;
    Ok(std::sync::Arc::new(
        move |id: &str| -> Result<OutsideRequest> {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()?
                .block_on(api.outside_take(&me, id))
        },
    ))
}

/// `riff outside list`, `riff outside approve` and `riff outside deny`
/// run as a person in a terminal: the person of the clone here.
fn person(server: &str, command: &str) -> Result<(Api, SessionUri)> {
    crate::forge::refuse_in_session(command)?;
    let me = crate::identity::person(&crate::identity::here(None)?, server)?;
    Ok((Api::new(server).signed_in(None)?, me))
}

/// `riff outside list`.
pub async fn list(server: &str) -> Result<String> {
    let (api, me) = person(server, "riff outside list")?;
    Ok(crate::text::outside_list(
        &api.outside_list(&me).await?.requests,
    ))
}

/// `riff outside approve ID` (`approve` true) or `riff outside deny ID`.
pub async fn decide(server: &str, id: &str, approve: bool) -> Result<String> {
    let command = if approve {
        "riff outside approve"
    } else {
        "riff outside deny"
    };
    let (api, me) = person(server, command)?;
    Ok(crate::text::outside_decided(
        &api.outside_decide(&me, id, approve).await?,
    ))
}
