//! Where a `riff-server` may listen.
//!
//! # Design
//!
//! A riff with no sign-in ([`crate::auth::Config::trusted`]) trusts each
//! caller. So it listens only on a loopback address, unless the person
//! gives `--insecure` (`RIFF_INSECURE`) (01M3JCE4ZD4DZCQ21FA69RT52D).
//! Its error names the settings of sign-in, `RIFF_OIDC_CLIENT_ID` and
//! `RIFF_OIDC_CLIENT_SECRET`, and `--insecure`
//! (01M3JZN1VEF73EPFE2FJY36EY4).
//!
//! A riff with sign-in listens only on a loopback address until it has
//! an owner. So the first person who signs in, the owner, signs in from
//! the machine of the server (01M3JN3AQMHZHT6JP3P6GM9PWZ). `--owner`
//! names the owner up front, for example for a cloud riff. With an
//! owner, a riff with sign-in listens on any address. `riff-server`
//! never terminates TLS: a proxy or the platform in front of it does
//! (01M3JCE51T84JZKJ0NR89TPDNY).
//!
//! ```mermaid
//! flowchart LR
//!     S[riff-server starts] --> T{no sign-in?}
//!     T -- no --> O{an owner?}
//!     O -- yes --> OK[listen]
//!     O -- no --> L2{loopback address?}
//!     L2 -- yes --> OK
//!     L2 -- no --> E
//!     T -- yes --> L{loopback address?}
//!     L -- yes --> OK
//!     L -- no --> I{--insecure?}
//!     I -- no --> E[refuse to start]
//!     I -- yes --> W[warn, then listen]
//! ```
//!
//! `riff-server` runs [`check()`] at start. The owner is in the state,
//! so `riff-server` checks again after it loads its bucket. Before that,
//! a bucket counts as an owner.
//!
//! # The port
//!
//! A [`Port`] holds the address from the start, and takes no connection
//! until [`Port::open`]. [`serve`] runs the server on it:
//!
//! - A server with a store opens its port after the load, and before it
//!   takes the lease (01M52JYGHHK5X0XS39VFT3ZGGM). A build that cannot
//!   load opens no port and takes no lease, so the old instance serves
//!   on.
//! - Until the server serves, each call gets 503 (01M52JYGM7WH565B7PCVBEJC17).
//!   `riff` tries it again (R132). So at an instance change, Cloud Run
//!   always has an open port to send the calls to: the old instance
//!   serves until the new one takes the lease, and the new one then
//!   serves after its wait.
//! - A server that stops for good, because another instance took the
//!   lease, closes its port at once. It takes no new call, so Cloud Run
//!   sends each call to the new instance. It exits when the checkpoint
//!   of the hand-over is written, at most [`crate::lease::Timing::exit_after`]
//!   after the stop (01M52JYGPX5ETP3HKR0S0Y0YCR).
//! - A SIGTERM before the end of the wait for the lease ends the start:
//!   the server closes its port and exits at once. It has no state to
//!   save yet (01M53NHK07KNXJ7FXS26NB1675). A lease that it took ends
//!   by its age, or the next instance takes it. So a second instance of
//!   an old revision, which waits for a lease of a newer revision
//!   ([`crate::lease::Lease::take_as`]), exits when Cloud Run stops it.
//! - A server with no store serves at once: its port opens when it
//!   serves.
//!
//! ```mermaid
//! sequenceDiagram
//!     participant C as Cloud Run
//!     participant O as old instance
//!     participant N as new instance
//!     participant S as store
//!     N->>S: load, with no lease
//!     Note over N: opens its port: 503 until it serves
//!     N->>S: write the lease (new ID), wait 15 s
//!     O->>S: read the lease: new ID
//!     Note over O: stops: closes its port
//!     C->>N: each call
//!     N-->>C: 503, riff tries again
//!     O->>S: write the checkpoint of the hand-over
//!     Note over O: exits
//!     N->>S: load the checkpoint and the chunks of the wait
//!     Note over N: serves
//!     C->>N: each call
//!     N-->>C: the reply
//! ```

use std::future::{Future, IntoFuture};
use std::net::SocketAddr;
use std::sync::{Arc, OnceLock};

use axum::Router;
use axum::extract::Request;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use riff_core::build::{self, Build};
use riff_state::store::StoreError;
use tokio::net::{TcpListener, TcpSocket};
use tower::ServiceExt;

use crate::{Loaded, Service};

/// The connections that wait for the server at one time.
const BACKLOG: u32 = 1024;

/// The address of a server that does not listen yet.
///
/// ```
/// # #[tokio::main] async fn main() -> std::io::Result<()> {
/// use riff_server::listen::Port;
/// use tokio::net::TcpStream;
///
/// let port = Port::reserve("127.0.0.1:0".parse().unwrap())?;
/// let addr = port.addr();
/// // The port is not open during the load.
/// assert!(TcpStream::connect(addr).await.is_err());
/// let listener = port.open()?;
/// assert_eq!(listener.local_addr()?, addr);
/// assert!(TcpStream::connect(addr).await.is_ok());
/// # Ok(()) }
/// ```
pub struct Port {
    socket: TcpSocket,
    addr: SocketAddr,
}

impl Port {
    /// Takes the address `listen`, and does not listen on it. It fails
    /// when another process has the address. With port 0, the OS gives
    /// a free port: see [`Port::addr`].
    pub fn reserve(listen: SocketAddr) -> std::io::Result<Port> {
        let socket = if listen.is_ipv4() {
            TcpSocket::new_v4()?
        } else {
            TcpSocket::new_v6()?
        };
        // As `TcpListener::bind`: a new start gets the address of a
        // server that stopped a moment ago.
        socket.set_reuseaddr(true)?;
        socket.bind(listen)?;
        let addr = socket.local_addr()?;
        Ok(Port { socket, addr })
    }

    /// The address, with the real port.
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// Opens the port: the server takes connections from now on.
    pub fn open(self) -> std::io::Result<TcpListener> {
        self.socket.listen(BACKLOG)
    }
}

/// Checks the listen address of a server. `trusted` is true for a riff
/// with no sign-in. `owned` is true when the riff has an owner. It
/// returns the warning to show at start, if there is one, or the error
/// that refuses the address.
///
/// ```
/// use riff_server::listen::check;
///
/// let open = "0.0.0.0:7878".parse().unwrap();
/// let local = "127.0.0.1:7878".parse().unwrap();
/// // No sign-in: only loopback, unless --insecure.
/// assert_eq!(check(local, true, false, false), Ok(None));
/// let error = check(open, true, false, false).unwrap_err();
/// assert!(error.contains("--insecure"));
/// assert!(error.contains("RIFF_OIDC_CLIENT_ID and RIFF_OIDC_CLIENT_SECRET"));
/// assert!(check(open, true, true, false).unwrap().unwrap().contains("any person"));
/// // With sign-in: only loopback until the riff has an owner.
/// assert_eq!(check(local, false, false, false), Ok(None));
/// assert!(check(open, false, false, false).unwrap_err().contains("--owner"));
/// assert_eq!(check(open, false, false, true), Ok(None));
/// ```
pub fn check(
    listen: SocketAddr,
    trusted: bool,
    insecure: bool,
    owned: bool,
) -> Result<Option<String>, String> {
    if listen.ip().is_loopback() {
        return Ok(None);
    }
    if !trusted {
        if owned {
            return Ok(None);
        }
        return Err(format!(
            "this riff has no owner yet, so it listens only on a loopback address, not on {listen}. \
             Sign in first on this machine with riff login: the first person is the owner. \
             Or name the owner with --owner EMAIL (RIFF_OWNER)."
        ));
    }
    if insecure {
        return Ok(Some(format!(
            "riff-server has no sign-in and listens on {listen} (--insecure): \
             each machine that can reach it can read, post and answer as any person"
        )));
    }
    Err(format!(
        "this riff has no sign-in, so it listens only on a loopback address, not on {listen}. \
         For sign-in, set RIFF_OIDC_CLIENT_ID and RIFF_OIDC_CLIENT_SECRET to the OAuth client \
         of your own OIDC app. Or add --insecure (RIFF_INSECURE) to let each machine that can \
         reach it read, post and answer as any person."
    ))
}

/// How a server starts in [`serve`].
pub enum Start {
    /// A server with no store: it serves at once.
    Now(Service),
    /// A server with a store, loaded with no lease: it opens its port,
    /// then takes the lease (01M52JYGHHK5X0XS39VFT3ZGGM).
    AfterLease(Box<Loaded>),
}

/// The text of the 503 of a server that does not serve yet
/// (01M52JYGM7WH565B7PCVBEJC17).
pub const STARTING: &str = "riff-server starts: it serves in a few seconds. Try again.";

/// The reply to each call while the server does not serve yet: 503,
/// with the build of this server. So `riff` sees a 503 of riff-server,
/// and tries the call again.
fn starting() -> Response {
    let mut reply = (
        StatusCode::SERVICE_UNAVAILABLE,
        [(header::RETRY_AFTER, "1")],
        STARTING,
    )
        .into_response();
    if let Ok(value) = header::HeaderValue::from_str(&Build::this().to_string()) {
        reply.headers_mut().insert(build::HEADER, value);
    }
    reply
}

/// Runs a server on `port` until it stops (see "The port" above).
///
/// - `start` gives the server. With [`Start::AfterLease`], the port
///   opens first, and each call gets 503 until the server serves.
/// - `check` runs after the first save of the server, before it serves.
///   An error stops the start: `main` checks the listen address there.
/// - At the end of `shutdown`, the server stops taking calls, saves and
///   ends its lease (R129). When `shutdown` ends before the end of the
///   wait for the lease, it closes the port and returns at once
///   (01M53NHK07KNXJ7FXS26NB1675).
/// - When the server stops for good (R141, 01M52JYGPX5ETP3HKR0S0Y0YCR),
///   it closes the port at once, and returns when the hand-over is
///   written, at most [`crate::lease::Timing::exit_after`] later. When
///   its lease ended by its age, it returns an error at once
///   (01M3X5TPBMF81TDVZ7Q4NVXBQX).
/// - When the start fails after the port opened, it closes the port, and
///   returns the error.
///
/// ```
/// # #[tokio::main] async fn main() -> std::io::Result<()> {
/// use riff_server::Service;
/// use riff_server::listen::{Port, Start, serve};
///
/// let port = Port::reserve("127.0.0.1:0".parse().unwrap())?;
/// let url = format!("http://{}/.well-known/oauth-authorization-server", port.addr());
/// let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
/// let server = tokio::spawn(serve(port, Start::Now(Service::default()), async {
///     let _ = stopped.await;
/// }, |_| Ok(())));
/// # let span = std::time::Instant::now();
/// let reply = loop {
///     if let Ok(reply) = reqwest::get(&url).await {
///         break reply;
///     }
/// #   assert!(span.elapsed().as_secs() < 30);
///     tokio::time::sleep(std::time::Duration::from_millis(10)).await;
/// };
/// assert_eq!(reply.status(), 200);
/// stop.send(()).unwrap();
/// server.await.unwrap()?;
/// # Ok(()) }
/// ```
pub async fn serve<D, C>(port: Port, start: Start, shutdown: D, check: C) -> std::io::Result<()>
where
    D: Future<Output = ()>,
    C: FnOnce(&Service) -> Result<(), String>,
{
    let mut shutdown = std::pin::pin!(shutdown);
    let routes: Arc<OnceLock<Router>> = Arc::default();
    let front = {
        let routes = routes.clone();
        Router::new().fallback(move |request: Request| {
            let routes = routes.clone();
            async move {
                match routes.get() {
                    Some(router) => match router.clone().oneshot(request).await {
                        Ok(reply) => reply,
                        Err(never) => match never {},
                    },
                    None => starting(),
                }
            }
        })
    };
    let (close, closed) = tokio::sync::watch::channel(false);
    let open = |port: Port| -> std::io::Result<tokio::task::JoinHandle<std::io::Result<()>>> {
        let listener = port.open()?;
        tracing::info!("riff-server listens on {}", listener.local_addr()?);
        let mut closed = closed.clone();
        let serving = axum::serve(listener, front.clone()).with_graceful_shutdown(async move {
            let _ = closed.wait_for(|closed| *closed).await;
        });
        Ok(tokio::spawn(serving.into_future()))
    };
    let failed = |error: StoreError| std::io::Error::other(error.to_string());
    let (service, mut serving) = match start {
        Start::Now(service) => {
            service.save().await.map_err(failed)?;
            check(&service)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e))?;
            let _ = routes.set(service.router());
            (service, open(port)?)
        }
        Start::AfterLease(loaded) => {
            let serving = open(port)?;
            // The server has no state to save before the end of its wait
            // for the lease: a SIGTERM then ends the start at once
            // (01M53NHK07KNXJ7FXS26NB1675).
            let leased = tokio::select! {
                leased = loaded.lease() => leased,
                () = &mut shutdown => {
                    let _ = close.send(true);
                    tracing::info!("stopping before it serves: it saves nothing");
                    return Ok(());
                }
            };
            let started = async {
                let service = leased.map_err(failed)?.serve().await.map_err(failed)?;
                // The owner of the settings is a record of the start:
                // wait for its write, so that the check reads it.
                service.save().await.map_err(failed)?;
                check(&service)
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e))?;
                Ok::<_, std::io::Error>(service)
            };
            match started.await {
                Ok(service) => {
                    let _ = routes.set(service.router());
                    (service, serving)
                }
                Err(error) => {
                    let _ = close.send(true);
                    return Err(error);
                }
            }
        }
    };
    tracing::info!("riff-server serves");
    tokio::select! {
        result = &mut serving => result.map_err(std::io::Error::other)?,
        () = &mut shutdown => {
            // Take no more calls, save each unsaved change, then exit (R129).
            let _ = close.send(true);
            tracing::info!("stopping: saving the state");
            service.shutdown().await.map_err(std::io::Error::other)
        }
        () = service.stopped() => {
            // Take no new call: Cloud Run sends each call to the new
            // instance (01M52JYGPX5ETP3HKR0S0Y0YCR).
            let _ = close.send(true);
            if service.lease_ended() {
                // A tool can have changed the log. Exit with an error,
                // so that a new instance loads the state from the store
                // (01M3X5TPBMF81TDVZ7Q4NVXBQX).
                return Err(std::io::Error::other(
                    "the lease of this instance ended by its age: its state in memory can \
                     be old",
                ));
            }
            tracing::info!("another instance serves: the port is closed");
            let exit_after = service.config().lease.exit_after;
            let ended = async {
                service.handed_over().await;
                let _ = serving.await;
            };
            if tokio::time::timeout(exit_after, ended).await.is_err() {
                tracing::warn!(
                    "exiting after {exit_after:?} with no end of the hand-over or of each call"
                );
            }
            tracing::info!("exiting: another instance serves");
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ipv6_loopback_needs_no_flag() {
        assert_eq!(
            check("[::1]:7878".parse().unwrap(), true, false, false),
            Ok(None)
        );
    }

    #[test]
    fn a_network_address_needs_the_flag() {
        let err = check("192.168.1.5:7878".parse().unwrap(), true, false, false).unwrap_err();
        assert!(err.contains("192.168.1.5:7878"), "{err}");
        assert!(err.contains("RIFF_INSECURE"), "{err}");
    }

    #[test]
    fn insecure_on_loopback_gives_no_warning() {
        assert_eq!(
            check("127.0.0.1:0".parse().unwrap(), true, true, false),
            Ok(None)
        );
    }

    /// --insecure is for a riff with no sign-in. It does not let a riff
    /// with sign-in and no owner listen on the network.
    #[test]
    fn insecure_does_not_replace_an_owner() {
        let err = check("192.168.1.5:7878".parse().unwrap(), false, true, false).unwrap_err();
        assert!(err.contains("RIFF_OWNER"), "{err}");
    }
}
