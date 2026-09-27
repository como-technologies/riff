//! Where a `riff-server` may listen.
//!
//! # Design
//!
//! A riff with no sign-in ([`crate::auth::Config::trusted`]) trusts each
//! caller. So it listens only on a loopback address, unless the person
//! gives `--insecure` (`RIFF_INSECURE`). A riff that requires sign-in
//! listens on any address (01M3JCE4ZD4DZCQ21FA69RT52D). `riff-server`
//! never terminates TLS: a proxy or the platform in front of it does
//! (01M3JCE51T84JZKJ0NR89TPDNY).
//!
//! ```mermaid
//! flowchart LR
//!     S[riff-server starts] --> T{no sign-in?}
//!     T -- no --> OK[listen]
//!     T -- yes --> L{loopback address?}
//!     L -- yes --> OK
//!     L -- no --> I{--insecure?}
//!     I -- no --> E[refuse to start]
//!     I -- yes --> W[warn, then listen]
//! ```
//!
//! `riff-server` and `riff-server install` both run [`check()`], so an
//! install never writes a service that cannot start.

use std::net::SocketAddr;

/// Checks the listen address of a server. `trusted` is true for a riff
/// with no sign-in. It returns the warning to show at start, if there
/// is one, or the error that refuses the address.
///
/// ```
/// use riff_server::listen::check;
///
/// let open = "0.0.0.0:7878".parse().unwrap();
/// let local = "127.0.0.1:7878".parse().unwrap();
/// // No sign-in: only loopback, unless --insecure.
/// assert_eq!(check(local, true, false), Ok(None));
/// assert!(check(open, true, false).unwrap_err().contains("--insecure"));
/// assert!(check(open, true, true).unwrap().unwrap().contains("any person"));
/// // With sign-in: any address, no warning.
/// assert_eq!(check(open, false, false), Ok(None));
/// ```
pub fn check(listen: SocketAddr, trusted: bool, insecure: bool) -> Result<Option<String>, String> {
    if !trusted || listen.ip().is_loopback() {
        return Ok(None);
    }
    if insecure {
        return Ok(Some(format!(
            "riff-server has no sign-in and listens on {listen} (--insecure): \
             each machine that can reach it can read, post and answer as any person"
        )));
    }
    Err(format!(
        "this riff has no sign-in, so it listens only on a loopback address, not on {listen}. \
         Add --insecure (RIFF_INSECURE) to let each machine that can reach it \
         read, post and answer as any person."
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ipv6_loopback_needs_no_flag() {
        assert_eq!(check("[::1]:7878".parse().unwrap(), true, false), Ok(None));
    }

    #[test]
    fn a_network_address_needs_the_flag() {
        let err = check("192.168.1.5:7878".parse().unwrap(), true, false).unwrap_err();
        assert!(err.contains("192.168.1.5:7878"), "{err}");
        assert!(err.contains("RIFF_INSECURE"), "{err}");
    }

    #[test]
    fn insecure_on_loopback_gives_no_warning() {
        assert_eq!(check("127.0.0.1:0".parse().unwrap(), true, true), Ok(None));
    }
}
