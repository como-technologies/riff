//! `riff device add --key NAME`, `riff device list` and `riff device
//! remove NAME`: the devices of a person (01M75NGT9Z0NSZTG97TBAMP7RS).
//!
//! A key is a FIDO2 / WebAuthn authenticator of the person: a security
//! key, a passkey on a phone, or the authenticator of the laptop. A
//! person can add two or more, so a lost one locks nobody out.
//!
//! ```mermaid
//! sequenceDiagram
//!     participant P as the person
//!     participant R as riff device add --key NAME
//!     participant S as riff-server
//!     participant B as browser
//!     R->>S: the start of the add
//!     S-->>R: the URL of the page of the add
//!     R->>B: open the page
//!     P->>B: press the button, then touch the authenticator
//!     B->>S: the reply of the authenticator
//!     R->>S: where is the add? (each second)
//!     S-->>R: added, or why not
//! ```
//!
//! - [`add_key`] gets an add from riff-server, shows the URL of its
//!   page, opens it in the browser, and waits for its end
//!   (01M75NH2RSEJ3K618M17PCH3ZZ). riff-server checks the reply of the
//!   authenticator: see `riff_server::devices`.
//! - `riff device list` prints [`crate::text::device_list`], and `riff
//!   device remove NAME` removes the device at once
//!   (01M75NH3625QATGDT6DK652ZK6).
//! - Each command refuses in an agent session and in a container, before
//!   each call ([`refusal`], 01M75NH30PNATYN33MXZQMVBW8): an
//!   authenticator is a device of the person, never of a session.

use std::time::Duration;

use anyhow::{Result, bail};
use riff_client::api::Api;
use riff_core::wire::KeyState;

use crate::text;

/// How often `riff device add` asks riff-server for the add, and for how
/// long at most: the life of an add, and a little more.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Wait {
    pub every: Duration,
    pub most: Duration,
}

impl Default for Wait {
    fn default() -> Self {
        Wait {
            every: Duration::from_secs(1),
            most: Duration::from_secs(5 * 60 + 30),
        }
    }
}

/// Why this process may not run `riff device`, or `None`
/// (01M75NH30PNATYN33MXZQMVBW8): an agent session, or a container.
///
/// ```
/// assert!(riff::device::refusal_for(true, false).unwrap().contains("a session never runs"));
/// assert!(riff::device::refusal_for(false, true).unwrap().contains("a container never runs"));
/// assert!(riff::device::refusal_for(false, false).is_none());
/// ```
pub fn refusal_for(agent: bool, container: bool) -> Option<String> {
    (agent || container).then(|| text::device_refused(container))
}

/// [`refusal_for`] this process.
pub fn refusal() -> Option<String> {
    refusal_for(crate::cycle::in_agent(), crate::cycle::in_container())
}

/// `riff device add --key NAME`: the add of the key `name` in the page
/// of riff-server. `show` gets the lines with the URL of the page, and
/// `open` opens the URL, for example in the browser. It waits for the
/// end of the add, and gives the name of the new key, or the reason of
/// riff-server.
pub async fn add_key(
    api: &Api,
    name: &str,
    show: impl FnOnce(&str),
    open: impl FnOnce(&str),
    wait: Wait,
) -> Result<String> {
    let started = api.key_start(name).await?;
    show(&text::device_add_opens(name, &started.url));
    open(&started.url);
    let span = tokio::time::Instant::now();
    loop {
        match api.key_wait(&started.ticket).await? {
            KeyState::Added { name } => return Ok(name),
            KeyState::Failed { reason } => bail!(text::device_add_failed(name, &reason)),
            KeyState::Waiting if span.elapsed() >= wait.most => {
                bail!(text::device_add_no_reply(name))
            }
            KeyState::Waiting => tokio::time::sleep(wait.every).await,
        }
    }
}
