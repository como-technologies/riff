//! The credential helpers of git, and the token of `gh` in a locked
//! keyring.
//!
//! # Design
//!
//! `gh auth setup-git` makes `gh auth git-credential` a credential helper
//! of git for GitHub. A `git fetch` of a private repository over HTTPS
//! then starts that `gh`, and `gh` reads its token from the OS keyring.
//! While the keyring is locked, that read asks for the unlock prompt, as
//! each `gh` call does. The riff daemon asks for no prompt
//! (01M77AB7XZV8KG8RRZ8C6R5A8X). So before each `git` of the daemon that
//! talks to a remote, [`keyring_open`] reads the URL of the remote
//! ([`urls`]) and the helpers that git starts for that URL ([`helpers`]).
//! When one of them is `gh` ([`is_gh`]), and the item of `gh` in the
//! keyring is locked, the daemon runs no such `git`. The error says to
//! unlock the keyring at the desktop, and the next try of the step runs
//! the `git` (01M78TSH2VNHA75RR1HXC5Z27G).
//!
//! ```mermaid
//! flowchart LR
//!     G["a git of the daemon:<br/>fetch, clone, push, ls-remote"] --> U["the URL of the remote"]
//!     U --> H{"a helper of git<br/>for the URL is gh?"}
//!     H -- no --> R["git runs"]
//!     H -- yes --> L{"the item of gh<br/>is locked?"}
//!     L -- no --> R
//!     L -- yes --> E["riff did not run git ...:<br/>unlock it at the desktop"]
//! ```
//!
//! A remote over SSH, a local path and a `file://` URL use no helper,
//! so their `git` runs. A command that a person types asks for the
//! prompt itself, so it runs each `git`, and reads no config for it.
//! [`run`] is the `git` of the daemon that talks to a remote: the check,
//! then the `git`.

use std::path::Path;
use std::process::{Command, Stdio};

use anyhow::Result;
use riff_client::secrets;

use crate::{pr, text};

/// The git config keys of the credential helpers: `credential.helper`,
/// and `credential.URL.helper` for the URLs that match `URL`.
const HELPER_KEYS: &str = r"^credential\.(.*\.)?helper$";

/// The program of `gh`, as a helper of git names it.
const GH: &str = "gh";

/// Runs `git ARGS` in `dir`, a call that talks to the remote `remote` (a
/// name or a URL), and gives its stdout. An error names `git` and its
/// stderr. In the riff daemon, it first checks that `git` would ask no
/// locked `gh` for a credential ([`keyring_open`]).
///
/// ```
/// let dir = tempfile::tempdir()?;
/// let git = |args: &[&str]| std::process::Command::new("git").args(args).current_dir(dir.path()).output();
/// git(&["init", "-q", "--bare", "origin.git"])?;
/// let origin = dir.path().join("origin.git");
/// let out = riff::git_credential::run(dir.path(), &origin.to_string_lossy(), &["ls-remote", &origin.to_string_lossy()])?;
/// assert_eq!(out, "");
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn run(dir: &Path, remote: &str, args: &[&str]) -> Result<String> {
    keyring_open(dir, remote, args)?;
    crate::run::output("git", dir, args)
}

/// Fails with the error of a locked keyring when `git ARGS` in `dir`, a
/// call that talks to the remote `remote`, would start `gh` for a
/// credential, and `gh` would read its token from a locked OS keyring, in
/// a process that asks for no prompt: the riff daemon
/// (01M78TSH2VNHA75RR1HXC5Z27G). The environment gives `gh` no token
/// ([`pr::token_given`]). A process that can ask for a prompt reads no
/// config of git here.
///
/// ```standalone_crate
/// # keyring_core::set_default_store(keyring_core::mock::Store::new().unwrap());
/// use keyring_core::{Entry, Error, mock};
/// use riff::git_credential::keyring_open;
///
/// let dir = tempfile::tempdir()?;
/// let git = |args: &[&str]| std::process::Command::new("git").args(args).current_dir(dir.path()).output();
/// git(&["init", "-q"])?;
/// git(&["remote", "add", "origin", "https://github.example/o/r.git"])?;
/// git(&["remote", "add", "ssh", "git@github.example:o/r.git"])?;
/// git(&["config", "credential.https://github.example.helper", "!/usr/bin/gh auth git-credential"])?;
/// // The keyring holds the token of gh, and keeps it locked.
/// let entry = Entry::new(riff::pr::GH_KEYRING, "")?;
/// entry.set_password("gho_not_real")?;
/// let cred: &mock::Cred = entry.as_any().downcast_ref().unwrap();
/// cred.set_error(Error::NoStorageAccess(Box::new(std::io::Error::other("locked"))));
/// # if !riff::pr::token_given() {
/// // A command of a person runs git: gh asks for the prompt itself.
/// keyring_open(dir.path(), "origin", &["fetch", "-q", "origin"])?;
/// // The daemon runs no git that would start gh, and says why.
/// riff_client::secrets::ask_no_prompt();
/// let error = keyring_open(dir.path(), "origin", &["fetch", "-q", "origin"]).unwrap_err();
/// assert!(riff_client::secrets::is_locked(&error), "{error:#}");
/// assert!(format!("{error:#}").starts_with("riff did not run git fetch -q origin: the OS keyring of "));
/// assert!(format!("{error:#}").contains("unlock it at the desktop"));
/// // A remote over SSH uses no helper: its git runs.
/// keyring_open(dir.path(), "ssh", &["fetch", "-q", "ssh"])?;
/// // The keyring is open again: the git runs.
/// keyring_open(dir.path(), "origin", &["fetch", "-q", "origin"])?;
/// # }
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn keyring_open(dir: &Path, remote: &str, args: &[&str]) -> Result<()> {
    if secrets::prompts() || pr::token_given() {
        return Ok(());
    }
    let push = subcommand(args) == Some("push");
    let asks_gh = urls(dir, remote, push)
        .iter()
        .any(|url| helpers(dir, url).iter().any(|helper| is_gh(helper)));
    if !asks_gh || !secrets::holds_locked(pr::GH_KEYRING, "") {
        return Ok(());
    }
    Err(anyhow::Error::msg(secrets::Locked::here()).context(text::git_not_run(&args.join(" "))))
}

/// The subcommand of `git ARGS`: the first word that is no option and
/// no value of `-c` or `-C`.
///
/// ```
/// use riff::git_credential::subcommand;
///
/// assert_eq!(subcommand(&["-c", "a=b", "-C", "x", "push", "origin"]), Some("push"));
/// assert_eq!(subcommand(&["fetch", "-q", "origin"]), Some("fetch"));
/// assert_eq!(subcommand(&["-q"]), None);
/// ```
pub fn subcommand<'a>(args: &[&'a str]) -> Option<&'a str> {
    let mut words = args.iter();
    while let Some(word) = words.next() {
        match *word {
            "-c" | "-C" => {
                words.next();
            }
            option if option.starts_with('-') => {}
            name => return Some(name),
        }
    }
    None
}

/// The URLs that git talks to for the remote `remote` of the clone `dir`:
/// its push URLs when `push`, else its URLs. The config `insteadOf` of
/// git applies. A remote that is a URL, and not a name, is that URL. A
/// `git` that does not run gives none.
///
/// ```
/// let dir = tempfile::tempdir()?;
/// let git = |args: &[&str]| std::process::Command::new("git").args(args).current_dir(dir.path()).output();
/// git(&["init", "-q"])?;
/// git(&["remote", "add", "origin", "https://github.example/o/r.git"])?;
/// git(&["config", "remote.origin.pushurl", "git@github.example:o/r.git"])?;
/// git(&["config", "url.https://github.example/.insteadOf", "gh:"])?;
/// let urls = |remote: &str, push: bool| riff::git_credential::urls(dir.path(), remote, push);
/// assert_eq!(urls("origin", false), ["https://github.example/o/r.git"]);
/// assert_eq!(urls("origin", true), ["git@github.example:o/r.git"]);
/// assert_eq!(urls("gh:o/x.git", false), ["https://github.example/o/x.git"]);
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn urls(dir: &Path, remote: &str, push: bool) -> Vec<String> {
    let mut args = vec!["remote", "get-url", "--all"];
    if push {
        args.push("--push");
    }
    args.push(remote);
    let found = git(dir, &args).or_else(|| git(dir, &["ls-remote", "--get-url", remote]));
    found
        .unwrap_or_default()
        .lines()
        .map(str::trim)
        .filter(|url| !url.is_empty())
        .map(str::to_owned)
        .collect()
}

/// The credential helpers that git starts for the URL `url` in `dir`, in
/// their order. Each config of git counts: of the system, of the person,
/// of the clone, and of the environment. A helper with an empty value
/// clears the list of the helpers before it, as in git. A URL that is
/// not `http` or `https`, for example SSH or a path, has no helper.
///
/// ```
/// let dir = tempfile::tempdir()?;
/// let git = |args: &[&str]| std::process::Command::new("git").args(args).current_dir(dir.path()).output();
/// git(&["init", "-q"])?;
/// git(&["config", "--add", "credential.https://github.example.helper", ""])?;
/// git(&["config", "--add", "credential.https://github.example.helper", "!/opt/test/gh auth git-credential"])?;
/// git(&["config", "credential.https://other.example.helper", "store"])?;
/// let helpers = |url: &str| riff::git_credential::helpers(dir.path(), url);
/// assert_eq!(helpers("https://github.example/o/r.git"), ["!/opt/test/gh auth git-credential"]);
/// let other = helpers("https://other.example/o/r.git");
/// assert!(other.contains(&"store".to_owned()) && !other.iter().any(|h| h.starts_with("!/opt/test/")));
/// assert_eq!(helpers("git@github.example:o/r.git"), Vec::<String>::new());
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn helpers(dir: &Path, url: &str) -> Vec<String> {
    let Some(target) = Url::parse(url).filter(Url::is_http) else {
        return Vec::new();
    };
    let found = git(dir, &["config", "-z", "--get-regexp", HELPER_KEYS]).unwrap_or_default();
    let mut helpers = Vec::new();
    for entry in found.split('\0') {
        let Some((key, value)) = entry.split_once('\n') else {
            continue;
        };
        let applies = match key
            .strip_prefix("credential.")
            .and_then(|k| k.strip_suffix(".helper"))
        {
            Some(pattern) => Url::parse(pattern).is_some_and(|p| p.matches(&target)),
            None => key == "credential.helper",
        };
        if !applies {
            continue;
        }
        if value.is_empty() {
            helpers.clear();
        } else {
            helpers.push(value.to_owned());
        }
    }
    helpers
}

/// True when the credential helper `helper` of git starts `gh`: a shell
/// command, or an absolute path, whose program is `gh`. A plain name
/// `NAME` is the program `git credential-NAME`.
///
/// ```
/// use riff::git_credential::is_gh;
///
/// assert!(is_gh("!gh auth git-credential"));
/// assert!(is_gh("!/usr/bin/gh auth git-credential"));
/// assert!(is_gh("!'/opt/my tools/gh' auth git-credential"));
/// assert!(is_gh("/usr/local/bin/gh auth git-credential"));
/// assert!(!is_gh("gh"));
/// assert!(!is_gh("store"));
/// assert!(!is_gh("!/usr/bin/ghx auth git-credential"));
/// assert!(!is_gh("/usr/lib/git-core/git-credential-libsecret"));
/// ```
pub fn is_gh(helper: &str) -> bool {
    let (shell, command) = match helper.strip_prefix('!') {
        Some(command) => (true, command.trim_start()),
        None => (false, helper.trim_start()),
    };
    let program = match command.chars().next() {
        Some(quote @ ('\'' | '"')) => command[1..].split(quote).next().unwrap_or_default(),
        _ => command.split_whitespace().next().unwrap_or_default(),
    };
    let program = Path::new(program);
    (shell || program.is_absolute()) && program.file_name().is_some_and(|name| name == GH)
}

/// The stdout of `git ARGS` in `dir`, or `None` when `git` fails.
fn git(dir: &Path, args: &[&str]) -> Option<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// The parts of a URL that select a credential helper of git: the scheme,
/// the host and the port. A pattern of a config can have no scheme. A
/// user and a path select nothing here, so a pattern with a path counts
/// for each path of its host: the daemon then waits more often, and
/// never asks a locked `gh`.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Url {
    scheme: Option<String>,
    host: String,
    port: Option<String>,
}

impl Url {
    /// The parts of `url`, or `None` for a URL with no host, for example
    /// `git@host:path` of SSH.
    fn parse(url: &str) -> Option<Url> {
        let (scheme, rest) = match url.split_once("://") {
            Some((scheme, rest)) => (Some(scheme.to_ascii_lowercase()), rest),
            None if url.contains(['@', ':']) => return None,
            None => (None, url),
        };
        let authority = rest.split('/').next().unwrap_or_default();
        let authority = authority
            .rsplit_once('@')
            .map_or(authority, |(_, host)| host);
        let (host, port) = match authority.rsplit_once(':') {
            Some((host, port)) if !port.is_empty() && port.bytes().all(|b| b.is_ascii_digit()) => {
                (host, Some(port.to_owned()))
            }
            _ => (authority, None),
        };
        if host.is_empty() {
            return None;
        }
        let default = match scheme.as_deref() {
            Some("http") => Some("80"),
            Some("https") => Some("443"),
            _ => None,
        };
        let port = port.filter(|p| Some(p.as_str()) != default);
        Some(Url {
            scheme,
            host: host.to_ascii_lowercase(),
            port,
        })
    }

    /// True for a URL of `http` or `https`: git asks a helper only for it.
    fn is_http(&self) -> bool {
        matches!(self.scheme.as_deref(), Some("http" | "https"))
    }

    /// True when the pattern `self` of a config selects the URL `url`:
    /// the same scheme, when the pattern has one, the same port, and the
    /// same host. A label `*` of the host of the pattern matches each
    /// label.
    fn matches(&self, url: &Url) -> bool {
        let scheme = self.scheme.is_none() || self.scheme == url.scheme;
        let labels = |host: &str| host.split('.').map(str::to_owned).collect::<Vec<_>>();
        let (pattern, host) = (labels(&self.host), labels(&url.host));
        let host = pattern.len() == host.len()
            && pattern.iter().zip(&host).all(|(p, h)| p == "*" || p == h);
        scheme && host && self.port == url.port
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(text: &str) -> Url {
        Url::parse(text).unwrap_or_else(|| panic!("no URL {text}"))
    }

    #[test]
    fn a_url_has_a_scheme_a_host_and_a_port_that_is_not_the_default() {
        let parsed = url("https://mona@GitHub.com:443/o/r.git");
        assert_eq!(
            parsed,
            Url {
                scheme: Some("https".into()),
                host: "github.com".into(),
                port: None
            }
        );
        assert_eq!(
            url("http://127.0.0.1:8080/o/r").port.as_deref(),
            Some("8080")
        );
        assert_eq!(url("github.com").scheme, None);
        assert_eq!(url("github.com/o").host, "github.com");
    }

    #[test]
    fn ssh_and_a_path_have_no_url_of_a_helper() {
        assert_eq!(Url::parse("git@github.com:o/r.git"), None);
        assert!(!url("ssh://git@github.com/o/r.git").is_http());
        assert!(Url::parse("file:///tmp/origin.git").is_none_or(|u| !u.is_http()));
        assert!(Url::parse("/tmp/origin.git").is_none_or(|u| !u.is_http()));
        assert!(url("https://github.com/o/r.git").is_http());
    }

    #[test]
    fn a_pattern_selects_a_url_by_its_scheme_host_and_port() {
        let target = url("https://github.com/o/r.git");
        assert!(url("https://github.com").matches(&target));
        assert!(url("https://github.com/other").matches(&target));
        assert!(url("github.com").matches(&target));
        assert!(url("https://*.com").matches(&target));
        assert!(!url("http://github.com").matches(&target));
        assert!(!url("https://gist.github.com").matches(&target));
        assert!(!url("https://github.com:8443").matches(&target));
        assert!(!url("https://*.github.com").matches(&target));
    }

    #[test]
    fn a_helper_of_each_url_counts_and_an_empty_one_clears_the_list() {
        let dir = tempfile::tempdir().unwrap();
        let out = Command::new("git")
            .args(["init", "-q"])
            .current_dir(dir.path())
            .output()
            .unwrap();
        assert!(out.status.success(), "git init: {out:?}");
        // The order of the sections is the order of the helpers.
        let config = dir.path().join(".git/config");
        let mut text = std::fs::read_to_string(&config).unwrap();
        text.push_str(
            "[credential]\n\thelper = cache\n\
             [credential \"http://127.0.0.1:9\"]\n\thelper =\n\thelper = !/tmp/fake/gh auth git-credential\n\
             [credential]\n\thelper = store\n",
        );
        std::fs::write(&config, text).unwrap();
        assert_eq!(
            helpers(dir.path(), "http://127.0.0.1:9/o/r.git"),
            ["!/tmp/fake/gh auth git-credential", "store"]
        );
        let other = helpers(dir.path(), "http://127.0.0.1:10/o/r.git");
        assert!(other.iter().all(|h| !is_gh(h)), "{other:?}");
        assert!(
            other.ends_with(&["cache".to_owned(), "store".to_owned()]),
            "{other:?}"
        );
    }

    #[test]
    fn a_command_of_a_person_runs_each_git() {
        // A unit test runs as a command of a person: it can ask for a
        // prompt.
        assert!(secrets::prompts());
        let none = Path::new("/nonexistent/riff-test");
        assert!(keyring_open(none, "origin", &["fetch", "origin"]).is_ok());
    }
}
