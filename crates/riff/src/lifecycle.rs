//! The life cycle of riff on a machine: `riff server` shows the riffs,
//! and `riff update` updates riff.
//!
//! # Design
//!
//! `riff` finds its server only in `--server` or `RIFF_SERVER`. With
//! neither, it uses the riff of this machine,
//! [`DEFAULT_SERVER`](crate::api::DEFAULT_SERVER). riff keeps no chosen
//! server in a file (01M3K0Q7X3FEKWZK0B3854C4RV). A person sets
//! `RIFF_SERVER` in the profile of the shell, so each new session uses
//! it.
//!
//! The riff of this machine runs in a terminal: `riff-server` starts it
//! and Ctrl-C stops it. So riff has no command to start or stop it.
//!
//! `riff server` asks the riff that `riff` uses and the riff of this
//! machine about themselves, with [`Api::probe`]. It shows each one on
//! one line (01M3K0Q854K18DGXJKQ427W586). See [`crate::text::server_view`].
//!
//! Only the network counts in the wait of a probe
//! (01M3MX598VTWZ02R7J6AYJB2E5). The two probes run at the same time.
//! riff reads the sign-ins from the keyring only after both probes end.
//! A keyring call blocks its task. A slow keyring in that task would use
//! up the wait of the other probe, and a riff that answers would show
//! "no answer".
//!
//! `riff update` does the update of a machine in one command
//! (01M3K0Q892KWM76R9DJC1P37JA). It installs a release: the git tag
//! `vX.Y.Z` of the crate version X.Y.Z (01M3MRMASMP59PKHAV92XSV7XE), not
//! the head of `main`. With no `--tag`, it installs the release that the
//! riff of `riff` runs, from the version of its build. So a newer tag
//! that nobody deployed yet does not break a machine. When `riff` uses
//! the riff of this machine, it installs the newest release tag, and
//! the person starts that riff again (01M3MRMAVVKJ5WS8GWCJHWH0R4):
//!
//! ```mermaid
//! sequenceDiagram
//!     participant U as riff update
//!     participant G as git ls-remote
//!     participant F as the riff of riff
//!     participant C as cargo
//!     participant R as the new riff
//!     participant S as riff-server of this machine
//!     alt --tag vX.Y.Z
//!         Note over U: that release
//!     else riff uses the riff of this machine
//!         U->>G: the tags v*
//!         G-->>U: the newest release vX.Y.Z
//!     else riff uses another riff
//!         U->>F: probe
//!         F-->>U: its build, with the version X.Y.Z
//!     end
//!     U->>C: install --locked --git REPOSITORY --tag vX.Y.Z riff riff-server
//!     U->>R: connect claude
//!     U->>S: probe
//!     alt it answers with another build than the new riff-server
//!         U-->>U: print "Stop riff-server and start it again."
//!     end
//! ```
//!
//! It does not restart `riff-server`: the server runs in a terminal of
//! the person.

use std::path::Path;
use std::process::Command;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use riff_core::build::Build;

use crate::api::{Api, Probe};
use crate::login;

/// The longest time that `riff server` and `riff update` wait for each
/// riff-server.
pub const PROBE_WAIT: Duration = Duration::from_secs(3);

/// Where `riff` got the server that it uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    /// `--server`.
    Flag,
    /// `RIFF_SERVER`.
    Env,
    /// Nothing named a server: the riff of this machine.
    Default,
}

/// What `riff` saw of one riff.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Seen {
    /// The URL of the riff.
    pub url: String,
    /// What it told, or why it did not answer.
    pub answer: Result<Probe, String>,
    /// True when this machine has a sign-in for it.
    pub signed_in: bool,
}

/// What `riff server` shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct View {
    /// Where the server of `riff` comes from.
    pub source: Source,
    /// The riff that `riff` uses.
    pub used: Seen,
    /// The riff of this machine, when `riff` uses another riff.
    pub local: Option<Seen>,
}

/// Asks the riff of `api` about itself, within [`PROBE_WAIT`].
async fn probe(api: &Api) -> Result<Probe, String> {
    api.probe(PROBE_WAIT).await.map_err(|e| format!("{e:#}"))
}

/// What `riff` saw of the riff at `url`: its `answer`, and the sign-in
/// of this machine from the keyring. The keyring call blocks, so call
/// it only after each probe ends (01M3MX598VTWZ02R7J6AYJB2E5).
fn seen(url: &str, answer: Result<Probe, String>) -> Seen {
    Seen {
        url: url.to_owned(),
        answer,
        signed_in: login::stored(url).is_ok_and(|s| s.is_some()),
    }
}

/// Looks at the riff at `server`, from `source`, and at the riff of
/// this machine at `local` when it is another riff (see [`same_riff`]).
/// `riff` passes [`DEFAULT_SERVER`](crate::api::DEFAULT_SERVER) as `local`.
///
/// ```
/// # #[tokio::main(flavor = "current_thread")]
/// # async fn main() {
/// # keyring_core::set_default_store(keyring_core::mock::Store::new().unwrap());
/// use riff::lifecycle::{Source, view};
///
/// let view = view("http://127.0.0.1:9", "http://127.0.0.1:7878", Source::Flag).await;
/// assert!(view.used.answer.is_err());
/// assert!(!view.used.signed_in);
/// assert_eq!(view.local.unwrap().url, "http://127.0.0.1:7878");
/// # }
/// ```
pub async fn view(server: &str, local: &str, source: Source) -> View {
    let used = Api::new(server);
    let (used, local) = if same_riff(server, local) {
        (seen(server, probe(&used).await), None)
    } else {
        let other = Api::new(local);
        let (a, b) = tokio::join!(probe(&used), probe(&other));
        (seen(server, a), Some(seen(local, b)))
    };
    View {
        source,
        used,
        local,
    }
}

/// The arguments of `cargo` that install riff and riff-server of the
/// release `tag` from the repository.
///
/// ```
/// let args = riff::lifecycle::install_args("v0.2.0");
/// assert_eq!(args[..3], ["install", "--locked", "--git"]);
/// assert_eq!(args[4..], ["--tag", "v0.2.0", "riff", "riff-server"]);
/// ```
pub fn install_args(tag: &str) -> [&str; 8] {
    [
        "install",
        "--locked",
        "--git",
        env!("CARGO_PKG_REPOSITORY"),
        "--tag",
        tag,
        env!("CARGO_PKG_NAME"),
        "riff-server",
    ]
}

/// The release tag of the crate version `version`
/// (01M3MRMASMP59PKHAV92XSV7XE).
///
/// ```
/// assert_eq!(riff::lifecycle::release_tag("0.2.0"), "v0.2.0");
/// ```
pub fn release_tag(version: &str) -> String {
    format!("v{version}")
}

/// `tag` when it is a release tag `vX.Y.Z`, for `riff update --tag`.
///
/// ```
/// use riff::lifecycle::parse_tag;
///
/// assert_eq!(parse_tag("v0.2.0").unwrap(), "v0.2.0");
/// assert_eq!(parse_tag("v10.0.12").unwrap(), "v10.0.12");
/// for bad in ["main", "0.2.0", "v0.2", "v0.2.0-rc1", "v0..1"] {
///     assert!(parse_tag(bad).is_err(), "{bad}");
/// }
/// ```
pub fn parse_tag(tag: &str) -> Result<String, String> {
    let parts: Vec<&str> = tag
        .strip_prefix('v')
        .unwrap_or_default()
        .split('.')
        .collect();
    let number = |p: &&str| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit());
    if parts.len() == 3 && parts.iter().all(number) {
        Ok(tag.to_owned())
    } else {
        Err(format!(
            "{tag} is not a release tag. Give vX.Y.Z, for example v0.2.0"
        ))
    }
}

/// The newest release tag in `ls_remote`, the output of
/// `git ls-remote --tags --refs`. It skips each tag that is not a
/// release tag, and compares the numbers, not the text.
///
/// ```
/// use riff::lifecycle::newest_tag;
///
/// let out = "a1\trefs/tags/v0.9.3\nb2\trefs/tags/v0.10.0\nc3\trefs/tags/v1.0.0-rc1\n";
/// assert_eq!(newest_tag(out).as_deref(), Some("v0.10.0"));
/// assert_eq!(newest_tag(""), None);
/// ```
pub fn newest_tag(ls_remote: &str) -> Option<String> {
    ls_remote
        .lines()
        .filter_map(|line| line.split_once("refs/tags/").map(|(_, tag)| tag.trim()))
        .filter_map(|tag| parse_tag(tag).ok())
        .max_by_key(|tag| {
            tag[1..]
                .split('.')
                .map(|n| n.parse::<u64>().unwrap_or_default())
                .collect::<Vec<_>>()
        })
}

/// The newest release tag of the repository, from `git ls-remote`
/// (01M3MRMAVVKJ5WS8GWCJHWH0R4).
pub fn newest_release() -> Result<String> {
    let out = Command::new("git")
        .args([
            "ls-remote",
            "--tags",
            "--refs",
            env!("CARGO_PKG_REPOSITORY"),
            "v*",
        ])
        .output()
        .context("cannot run git ls-remote")?;
    if !out.status.success() {
        bail!(
            "git ls-remote failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    newest_tag(&String::from_utf8_lossy(&out.stdout)).with_context(|| {
        format!(
            "{} has no release tag. Name one: riff update --tag vX.Y.Z",
            env!("CARGO_PKG_REPOSITORY")
        )
    })
}

/// The release that the riff at `server` runs: the release tag of the
/// version of its build (01M3MRMAVVKJ5WS8GWCJHWH0R4). An error when it
/// does not answer, or names no build.
pub async fn server_release(server: &str) -> Result<String> {
    let probe = Api::new(server).probe(PROBE_WAIT).await.with_context(|| {
        format!(
            "cannot find the release of the riff at {server}. \
                 Name one: riff update --tag vX.Y.Z"
        )
    })?;
    match probe.build {
        Some(build) => Ok(release_tag(&build.version)),
        None => bail!(
            "the riff at {server} names no build, so riff cannot find its release. \
             Name one: riff update --tag vX.Y.Z"
        ),
    }
}

/// True when `url` names this machine: a riff that the person can
/// restart.
///
/// ```
/// use riff::lifecycle::is_loopback;
///
/// assert!(is_loopback("http://127.0.0.1:7878"));
/// assert!(is_loopback("http://localhost:9000"));
/// assert!(is_loopback("http://[::1]:7878"));
/// assert!(!is_loopback("http://first:7878"));
/// assert!(!is_loopback("https://riff.example.com"));
/// ```
pub fn is_loopback(url: &str) -> bool {
    loopback_host(address(url).0)
}

/// True when `a` and `b` name one riff: the same port, and the same
/// host or two names of this machine. So `localhost` and `127.0.0.1`
/// are one riff (01M3MNT26K77E4RDHH42SB5AEG).
///
/// ```
/// use riff::lifecycle::same_riff;
///
/// assert!(same_riff("http://localhost:7878", "http://127.0.0.1:7878"));
/// assert!(same_riff("http://[::1]:7878", "http://127.0.0.1:7878"));
/// assert!(same_riff("http://first:7878", "http://FIRST:7878/"));
/// assert!(same_riff("https://riff.example.com", "https://riff.example.com:443"));
/// assert!(!same_riff("http://localhost:9000", "http://127.0.0.1:7878"));
/// assert!(!same_riff("http://first:7878", "http://127.0.0.1:7878"));
/// ```
pub fn same_riff(a: &str, b: &str) -> bool {
    let (host_a, port_a) = address(a);
    let (host_b, port_b) = address(b);
    port_a == port_b
        && (host_a.eq_ignore_ascii_case(host_b) || (loopback_host(host_a) && loopback_host(host_b)))
}

/// The host of `url`, with no brackets, and its port: the port of the
/// URL, else 443 for `https` and 80 for each other scheme.
fn address(url: &str) -> (&str, Option<u16>) {
    let (scheme, rest) = url.split_once("://").unwrap_or(("http", url));
    let authority = rest.split('/').next().unwrap_or_default();
    let (host, port) = match authority.strip_prefix('[') {
        Some(v6) => {
            let (host, rest) = v6.split_once(']').unwrap_or((v6, ""));
            (host, rest.strip_prefix(':'))
        }
        None => match authority.split_once(':') {
            Some((host, port)) => (host, Some(port)),
            None => (authority, None),
        },
    };
    let port = match port {
        Some(port) => port.parse().ok(),
        None if scheme == "https" => Some(443),
        None => Some(80),
    };
    (host, port)
}

/// True when `host` names this machine.
fn loopback_host(host: &str) -> bool {
    host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<std::net::IpAddr>()
            .is_ok_and(|ip| ip.is_loopback())
}

/// The build in the `--version` line of `riff-server`.
///
/// ```
/// use riff::lifecycle::version_build;
///
/// let b = version_build("riff-server 0.4.1 929605821e54 2026-09-27T22:03:01Z\n").unwrap();
/// assert_eq!(b.commit, "929605821e54");
/// assert!(version_build("riff-server").is_none());
/// ```
pub fn version_build(line: &str) -> Option<Build> {
    line.trim().split_once(' ')?.1.parse().ok()
}

/// Updates riff on this machine (01M3K0Q892KWM76R9DJC1P37JA): installs
/// the binaries of a release with `cargo`: `tag`, else the newest
/// release ([`newest_release`]) when `server` is `local`, the riff of
/// this machine, else the release that `server` runs
/// ([`server_release`]). Then it updates the plugin with the new
/// `riff connect claude --claude CLAUDE`, and looks for an old riff with
/// [`old_riff`]. It returns the last words for the person. `riff` passes
/// [`DEFAULT_SERVER`](crate::api::DEFAULT_SERVER) as `local`.
pub async fn update(
    cargo: &Path,
    claude: &Path,
    tag: Option<&str>,
    server: &str,
    local: &str,
) -> Result<String> {
    let tag = match tag {
        Some(tag) => tag.to_owned(),
        None if same_riff(server, local) => newest_release()?,
        None => server_release(server).await?,
    };
    run(
        Command::new(cargo).args(install_args(&tag)),
        "cargo install",
    )?;
    run(
        Command::new("riff")
            .args(["connect", "claude", "--claude"])
            .arg(claude),
        "riff connect claude",
    )?;
    let out = Command::new("riff-server")
        .arg("--version")
        .output()
        .context("cannot run riff-server --version")?;
    let new = version_build(&String::from_utf8_lossy(&out.stdout));
    let old = old_riff(new.as_ref(), server, local).await;
    Ok(crate::text::updated(old.as_deref()))
}

/// The riff of this machine when it runs another build than `new`, the
/// build of the new riff-server. That riff is `server` when it is on
/// this machine, else `local`, also when `RIFF_SERVER` names a remote
/// riff (01M3K0Q892KWM76R9DJC1P37JA).
pub async fn old_riff(new: Option<&Build>, server: &str, local: &str) -> Option<String> {
    let url = if is_loopback(server) { server } else { local };
    let running = probe(&Api::new(url)).await.ok()?.build?;
    (!new?.matches(&running)).then(|| url.to_owned())
}

/// Runs `command` with the terminal of the person. An error when it
/// fails.
fn run(command: &mut Command, name: &str) -> Result<()> {
    let status = command
        .status()
        .with_context(|| format!("cannot run {name}"))?;
    if !status.success() {
        bail!("{name} failed: {status}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::DEFAULT_SERVER;

    #[test]
    fn a_path_after_the_host_does_not_hide_loopback() {
        assert!(is_loopback("http://127.0.0.1:7878/v1"));
        assert!(is_loopback("127.0.0.1"));
        assert!(!is_loopback("http://127.example.com"));
    }

    #[tokio::test]
    async fn the_default_server_is_seen_once() {
        let view = view(DEFAULT_SERVER, DEFAULT_SERVER, Source::Default).await;
        assert_eq!(view.used.url, DEFAULT_SERVER);
        assert!(view.local.is_none());
    }

    #[tokio::test]
    async fn localhost_and_127_0_0_1_are_seen_once() {
        let view = view("http://localhost:7878", DEFAULT_SERVER, Source::Env).await;
        assert_eq!(view.used.url, "http://localhost:7878");
        assert!(view.local.is_none());
    }

    #[test]
    fn an_address_with_no_port_takes_the_port_of_its_scheme() {
        assert_eq!(address("http://first"), ("first", Some(80)));
        assert_eq!(address("https://first/v1"), ("first", Some(443)));
        assert_eq!(address("http://[::1]:9000"), ("::1", Some(9000)));
        assert_eq!(address("http://[::1]"), ("::1", Some(80)));
        assert_eq!(address("http://first:port"), ("first", None));
    }

    #[tokio::test]
    async fn another_server_is_seen_with_the_riff_of_this_machine() {
        let view = view("http://127.0.0.1:9", DEFAULT_SERVER, Source::Flag).await;
        assert!(view.used.answer.is_err());
        assert_eq!(view.local.map(|l| l.url).as_deref(), Some(DEFAULT_SERVER));
    }
}
