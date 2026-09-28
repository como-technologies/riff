//! Where a `riff-server` may listen.
//!
//! # Design
//!
//! A riff with no sign-in ([`crate::auth::Config::trusted`]) trusts each
//! caller. So it listens only on a loopback address, unless the person
//! gives `--insecure` (`RIFF_INSECURE`) (01M3JCE4ZD4DZCQ21FA69RT52D).
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
//! `riff-server` and `riff-server install` both run [`check()`], so an
//! install never writes a service that cannot start. The owner is in
//! the state, so `riff-server` checks again after it loads its bucket.
//! Before that, and in `install`, a bucket counts as an owner.

use std::net::SocketAddr;

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
/// assert!(check(open, true, false, false).unwrap_err().contains("--insecure"));
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
         Add --insecure (RIFF_INSECURE) to let each machine that can reach it \
         read, post and answer as any person."
    ))
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
