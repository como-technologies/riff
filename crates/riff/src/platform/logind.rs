//! logind: the notices of a stop of the host on Linux (#2164,
//! 01M7EWYHX092A3QWM38RNXYWMA). See [`crate::host_stop`] for the design.
//!
//! The daemon talks to logind on the system bus. It reads the longest
//! delay of a lock (`InhibitDelayMaxUSec`), listens for
//! `PrepareForShutdown`, and takes a delay lock for `shutdown`
//! ([`INHIBIT`]). The lock is a file descriptor: its close lets the stop
//! go on. A delay lock needs no password: the rules of polkit of logind
//! give it to each user.

use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use futures::StreamExt;
use zbus::proxy::{Builder, CacheProperties, SignalStream};
use zbus::zvariant::OwnedFd;
use zbus::{Connection, Proxy};

use crate::host_stop::Notices;

/// The name of logind on the system bus.
pub const LOGIND: &str = "org.freedesktop.login1";

/// The object of the manager of logind.
pub const PATH: &str = "/org/freedesktop/login1";

/// The interface of the manager of logind.
pub const MANAGER: &str = "org.freedesktop.login1.Manager";

/// The arguments of `Inhibit`: what the lock holds, who holds it, why,
/// and its mode. `systemd-inhibit --list` shows them.
pub const INHIBIT: (&str, &str, &str, &str) = (
    "shutdown",
    "riff",
    "the work sessions of riff push their work",
    "delay",
);

/// The manager of logind, and its signal `PrepareForShutdown`.
pub struct Logind {
    manager: Proxy<'static>,
    notices: SignalStream<'static>,
}

impl Logind {
    /// Connects to logind on the system bus, and listens for
    /// `PrepareForShutdown`. It gives the longest delay of a lock too.
    pub async fn connect() -> Result<(Logind, Duration)> {
        let conn = Connection::system()
            .await
            .context("riff cannot reach the system bus")?;
        let manager: Proxy<'static> = Builder::new(&conn)
            .destination(LOGIND)?
            .path(PATH)?
            .interface(MANAGER)?
            .cache_properties(CacheProperties::No)
            .build()
            .await?;
        let delay: u64 = manager
            .get_property("InhibitDelayMaxUSec")
            .await
            .context("logind does not say the delay of a lock")?;
        let notices = manager.receive_signal("PrepareForShutdown").await?;
        Ok((Logind { manager, notices }, Duration::from_micros(delay)))
    }
}

impl Notices for Logind {
    type Lock = OwnedFd;

    async fn lock(&mut self) -> Result<OwnedFd> {
        self.manager
            .call("Inhibit", &INHIBIT)
            .await
            .context("logind refused the delay lock")
    }

    async fn next(&mut self) -> Result<bool> {
        let notice = self
            .notices
            .next()
            .await
            .ok_or_else(|| anyhow!("the system bus ended the signals of logind"))?;
        Ok(notice.body().deserialize::<bool>()?)
    }
}
