//! The profile of each role: what a process of the role may do.
//!
//! # Design
//!
//! riff runs four roles: three run an AI (the **lead**, a **worker**, a
//! **verifier**), and the **test run** runs the tests of one of them. A
//! [`Profile`] says what a process of a role may do: the paths that it
//! reads, the paths that it writes, the network, and the rights on the
//! forge. [`Profile::of`] builds it from the [`Session`]: the paths of
//! one session. The profile is the one source: the sandbox of a session
//! (#607), the namespaces of a test run (#612), the forge token (#610)
//! and the permission rules of Claude Code (#613) read it
//! (01M4BPK6V66QNPQJ6RBRE10WH9). The profile of each role:
//! lead 01M4BPK72ZBZABCTWS9YM1M9QX, worker 01M4BPK7AKTJD9Y9WVJQKTQY6M,
//! verifier 01M4BPK7JA9V5PXCYZ01G0KBZT, test run
//! 01M4BPK7SN7J16KPB3J743CW2B and 01M4BTG79MW1DJFGG287X5PH9G.
//!
//! | | lead | worker | verifier | test run |
//! |---|---|---|---|---|
//! | Write | the clone, its worktrees, the riff state, its temp, its Claude folder | its worktree, its target, the git dir of the clone, the riff state, its temp, its Claude folder | its verify worktree, its target, the git dir of the clone, the riff state, its temp, its Claude folder | its temp, its target |
//! | Read | the system, its tools, and what it writes | the same | the same | the same, its worktree and the git dir of the clone |
//! | Read the home of the person | no, except the paths above | no | no | no |
//! | Network | riff server, forge, registries, model | the same | the same | loopback only |
//! | Keyring, D-Bus and systemd of the person | no | no | no | no |
//! | Forge | read, plan, comment, push, pull request | read, comment, push, pull request | read, comment, verify status | none |
//!
//! - A role has no field for the keyring or the D-Bus of the person:
//!   no profile can grant them (01M4BPK80NK50S2V26Z8BDT0XM).
//! - No profile reaches a bus of systemd: the user bus, the private
//!   socket of the user manager, the system bus
//!   (01M4C2PY1DRWVWENJBM42D60M9). So no process of a sandbox calls
//!   systemd. Only `riff workers run`, outside each sandbox, does
//!   ([`crate::limits`], 01M4C2PXZ5WNE4C2CJW2HABPY0).
//! - [`Profile::of`] refuses a session whose paths give the home of the
//!   person, a folder above it, or a place of a secret
//!   ([`Session::secrets`]): the keyring, the D-Bus socket, the keys of
//!   SSH and GnuPG, the sign-in of `gh`, the key of the GitHub App of
//!   riff ([`crate::forge`]). So the home of a person that
//!   is a git repository can never be the clone.
//! - [`Profile::of`] refuses a path that is not absolute or that has a
//!   `..` component (01M4BR61PPQV7JJE5Y2G9Q90AF): it compares
//!   components and does not resolve them. The apply step (#607)
//!   resolves each symlink on the disk before it grants a path.
//! - Each role writes its temp folder, so no role writes nothing
//!   ([`Writes`]).
//! - A git worktree keeps its objects and its refs in the git dir of
//!   the clone. So a worker and a verifier write that dir, and not the
//!   rest of the clone.
//! - A test run has the loopback network only, in a network namespace
//!   of its own (#612). Each other role connects only to the TCP ports
//!   of [`Network::ports`].
//!
//! ```mermaid
//! flowchart LR
//!     S["Session: the paths of one session"] --> P["Profile::of(role, session)"]
//!     P --> L["lead: riff starts it (#608)"]
//!     P --> W["worker: riff workers run"]
//!     P --> V["verifier: riff workers run"]
//!     P --> T["test run: just test (#612)"]
//!     L & W & V --> K["Landlock: files and ports (#607)"]
//!     L & W & V --> G["forge token of the role (#610)"]
//!     L & W & V --> R["permission rules of Claude Code (#613)"]
//!     T --> N["namespaces: home, /tmp, processes, loopback (#612)"]
//! ```
//!
//! ```
//! use riff::profile::{Endpoint, Profile, Right, Role, Session};
//! use std::path::Path;
//!
//! let session = Session {
//!     home: "/home/ada".into(),
//!     runtime: "/run/user/1000".into(),
//!     clone: "/home/ada/src/app".into(),
//!     worktree: "/home/ada/src/app/.claude/worktrees/issue-12".into(),
//!     target: "/home/ada/src/app/.claude/worktrees/issue-12/target".into(),
//!     temp: "/home/ada/.cache/riff/tmp/s1".into(),
//!     claude: "/home/ada/.claude".into(),
//!     state: "/run/user/1000/riff".into(),
//!     tools: vec!["/home/ada/.cargo".into(), "/home/ada/.rustup".into()],
//!     server: Endpoint::of_url("https://riff.example.com").unwrap(),
//! };
//! let worker = Profile::of(Role::Worker, &session)?;
//! assert!(worker.writes(Path::new("/home/ada/src/app/.claude/worktrees/issue-12/src/lib.rs")));
//! assert!(worker.writes(Path::new("/home/ada/src/app/.git/objects/ab")));
//! assert!(!worker.writes(Path::new("/home/ada/src/app/README.md")));
//! assert!(!worker.reads(Path::new("/home/ada/.local/share/keyrings/login.keyring")));
//! assert!(worker.may(Right::Push) && !worker.may(Right::Verify));
//!
//! let test = Profile::of(Role::TestRun, &session)?;
//! assert!(test.network().ports().is_empty());
//! assert!(test.rights().is_empty());
//!
//! // The home of the person is never a path of a profile.
//! let bad = Session { clone: "/home/ada".into(), ..session };
//! assert!(Profile::of(Role::Lead, &bad).is_err());
//! # Ok::<(), riff::profile::Refused>(())
//! ```

use std::fmt;
use std::path::{Path, PathBuf};

/// A role that riff runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Role {
    /// The lead of a person in a repository: it plans and conducts.
    Lead,
    /// A worker: it does one item in its worktree.
    Worker,
    /// A verifier: it checks the work of another session.
    Verifier,
    /// The test run of a session: `just test`, `cargo test`.
    TestRun,
}

impl Role {
    /// Each role, in the order of the table of the module.
    pub const ALL: [Role; 4] = [Role::Lead, Role::Worker, Role::Verifier, Role::TestRun];

    /// The name of the role, as the book and the requirements say it.
    ///
    /// ```
    /// use riff::profile::Role;
    ///
    /// assert_eq!(Role::TestRun.name(), "test run");
    /// ```
    pub fn name(self) -> &'static str {
        match self {
            Role::Lead => "lead",
            Role::Worker => "worker",
            Role::Verifier => "verifier",
            Role::TestRun => "test run",
        }
    }

    /// The rights of the role on the forge.
    pub fn rights(self) -> &'static [Right] {
        match self {
            Role::Lead => &[
                Right::Read,
                Right::Plan,
                Right::Comment,
                Right::Push,
                Right::PullRequest,
            ],
            Role::Worker => &[Right::Read, Right::Comment, Right::Push, Right::PullRequest],
            Role::Verifier => &[Right::Read, Right::Comment, Right::Verify],
            Role::TestRun => &[],
        }
    }
}

impl fmt::Display for Role {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// A right on the forge. The GitHub App gives a token with only the
/// rights of the role (#610).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Right {
    /// Read the code, the issues and the pull requests.
    Read,
    /// Plan the waves: make a milestone, put an issue in it, file an issue.
    Plan,
    /// Comment on an issue or a pull request.
    Comment,
    /// Push a branch that is not the default branch.
    Push,
    /// Open a pull request and turn on its auto-merge.
    PullRequest,
    /// Set the status `riff/verify` of a commit.
    Verify,
}

/// A host and a TCP port.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Endpoint {
    /// The name or the address of the host.
    pub host: String,
    /// The TCP port.
    pub port: u16,
}

impl Endpoint {
    /// The endpoint of a URL, with the default port of its scheme.
    /// `None` when the URL has no host or no known port.
    ///
    /// ```
    /// use riff::profile::Endpoint;
    ///
    /// let e = Endpoint::of_url("https://riff.example.com/x").unwrap();
    /// assert_eq!((e.host.as_str(), e.port), ("riff.example.com", 443));
    /// assert_eq!(Endpoint::of_url("http://127.0.0.1:7878").unwrap().port, 7878);
    /// assert_eq!(Endpoint::of_url("not a url"), None);
    /// ```
    pub fn of_url(url: &str) -> Option<Self> {
        let url = reqwest::Url::parse(url).ok()?;
        Some(Self {
            host: url.host_str()?.to_owned(),
            port: url.port_or_known_default()?,
        })
    }

    fn https(host: &str) -> Self {
        Self {
            host: host.to_owned(),
            port: 443,
        }
    }
}

/// The hosts of the forge: GitHub, its API and its files.
pub const FORGE: [&str; 3] = [
    "github.com",
    "api.github.com",
    "objects.githubusercontent.com",
];

/// The hosts of the package registries: crates.io, its index and its
/// files.
pub const REGISTRIES: [&str; 3] = ["crates.io", "index.crates.io", "static.crates.io"];

/// The host of the model API that the agent of an AI role calls.
pub const MODEL: &str = "api.anthropic.com";

/// The network of a role.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Network {
    /// Only the loopback interface, in a network namespace of its own:
    /// the test servers of the run (#612).
    Loopback,
    /// Out to the riff server, the forge, the registries and the model,
    /// each over HTTPS, and to the riff server on its port.
    Out {
        /// The riff server of the session.
        server: Endpoint,
    },
}

impl Network {
    /// Each endpoint that the role may connect to. Empty for
    /// [`Network::Loopback`].
    pub fn endpoints(&self) -> Vec<Endpoint> {
        match self {
            Network::Loopback => vec![],
            Network::Out { server } => std::iter::once(server.clone())
                .chain(FORGE.iter().map(|h| Endpoint::https(h)))
                .chain(REGISTRIES.iter().map(|h| Endpoint::https(h)))
                .chain(std::iter::once(Endpoint::https(MODEL)))
                .collect(),
        }
    }

    /// The TCP ports that the role may connect to, sorted, each once.
    /// Landlock limits the ports, not the hosts (#607).
    ///
    /// ```
    /// use riff::profile::{Endpoint, Network};
    ///
    /// let server = Endpoint::of_url("http://127.0.0.1:7878").unwrap();
    /// assert_eq!(Network::Out { server }.ports(), [443, 7878]);
    /// assert!(Network::Loopback.ports().is_empty());
    /// ```
    pub fn ports(&self) -> Vec<u16> {
        let mut ports: Vec<u16> = self.endpoints().iter().map(|e| e.port).collect();
        ports.sort_unstable();
        ports.dedup();
        ports
    }
}

/// The paths of one session. riff builds the [`Profile`] of the session
/// from them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    /// The home of the person. No role reads it as a whole.
    pub home: PathBuf,
    /// The runtime folder of the person (`XDG_RUNTIME_DIR`): it holds
    /// the D-Bus socket and the keyring socket.
    pub runtime: PathBuf,
    /// The main clone of the repository.
    pub clone: PathBuf,
    /// The worktree of the session: the clone for the lead.
    pub worktree: PathBuf,
    /// The cargo target folder of the worktree.
    pub target: PathBuf,
    /// The temp folder of the session (see [`temp`](crate::temp)).
    pub temp: PathBuf,
    /// The Claude Code folder of the session.
    pub claude: PathBuf,
    /// The local files of riff (see [`local::dir`](crate::local::dir)).
    pub state: PathBuf,
    /// The other paths that each role reads: the toolchain, the
    /// binaries of riff and Claude Code, the settings of riff.
    pub tools: Vec<PathBuf>,
    /// The riff server.
    pub server: Endpoint,
}

impl Session {
    /// The places of the secrets of the person. No profile reads or
    /// writes them, a path in them, or a folder above them. The buses
    /// of systemd are here too: the user bus, the private socket of the
    /// user manager and the system bus. So no process of a sandbox
    /// calls systemd (01M4C2PY1DRWVWENJBM42D60M9).
    ///
    /// ```
    /// use riff::profile::{Endpoint, Session};
    /// use std::path::Path;
    ///
    /// let s = Session {
    ///     home: "/h".into(),
    ///     runtime: "/run/user/7".into(),
    ///     clone: "/h/app".into(),
    ///     worktree: "/h/app".into(),
    ///     target: "/h/app/target".into(),
    ///     temp: "/h/tmp".into(),
    ///     claude: "/h/.claude".into(),
    ///     state: "/run/user/7/riff".into(),
    ///     tools: vec![],
    ///     server: Endpoint::of_url("http://127.0.0.1:7878").unwrap(),
    /// };
    /// assert!(s.secrets().contains(&Path::new("/run/user/7/bus").to_path_buf()));
    /// assert!(s.secrets().contains(&Path::new("/h/.local/share/keyrings").to_path_buf()));
    /// assert!(s.secrets().contains(&Path::new("/run/user/7/systemd").to_path_buf()));
    /// assert!(s.secrets().contains(&Path::new("/run/dbus").to_path_buf()));
    /// assert!(s.secrets().contains(&Path::new("/h/.config/riff/forge").to_path_buf()));
    /// ```
    pub fn secrets(&self) -> Vec<PathBuf> {
        let home = |p: &str| self.home.join(p);
        let runtime = |p: &str| self.runtime.join(p);
        vec![
            home(".local/share/keyrings"),
            home(".ssh"),
            home(".gnupg"),
            home(".config/gh"),
            home(".config/riff/forge"),
            runtime("bus"),
            runtime("systemd"),
            runtime("keyring"),
            runtime("gnupg"),
            SYSTEM_BUS.into(),
        ]
    }
}

/// The paths that a role writes. Each role writes its temp folder, so
/// a role that writes nothing cannot exist.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Writes {
    temp: PathBuf,
    more: Vec<PathBuf>,
}

impl Writes {
    /// Each path, the temp folder first.
    pub fn paths(&self) -> impl Iterator<Item = &Path> {
        std::iter::once(self.temp.as_path()).chain(self.more.iter().map(PathBuf::as_path))
    }
}

/// What a process of a role may do. Only [`Profile::of`] makes one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Profile {
    role: Role,
    writes: Writes,
    reads: Vec<PathBuf>,
    network: Network,
}

/// The folder of the socket of the D-Bus system bus.
pub const SYSTEM_BUS: &str = "/run/dbus";

/// The folders of the system that each role reads.
pub const SYSTEM: [&str; 10] = [
    "/usr", "/bin", "/sbin", "/lib", "/lib64", "/etc", "/opt", "/dev", "/proc", "/sys",
];

/// Why [`Profile::of`] makes no profile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refused {
    /// A path of the session is not absolute.
    Relative(PathBuf),
    /// A path of the session has a `..` component. A compare of
    /// components does not resolve it, so riff refuses it.
    Up(PathBuf),
    /// A path of the profile is the home of the person or a folder
    /// above it.
    Home(PathBuf),
    /// A path of the profile is a place of a secret, is in one, or is a
    /// folder above one (see [`Session::secrets`]).
    Secret(PathBuf),
}

impl fmt::Display for Refused {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Refused::Relative(p) => write!(f, "the path {} is not absolute", p.display()),
            Refused::Up(p) => write!(f, "the path {} has a `..` component", p.display()),
            Refused::Home(p) => write!(
                f,
                "the path {} gives the home of the person; no role may have it",
                p.display()
            ),
            Refused::Secret(p) => write!(
                f,
                "the path {} gives a secret of the person; no role may have it",
                p.display()
            ),
        }
    }
}

impl std::error::Error for Refused {}

impl Profile {
    /// The profile of `role` for `session`. It refuses a session whose
    /// paths give the home of the person or a secret ([`Refused`]).
    pub fn of(role: Role, session: &Session) -> Result<Self, Refused> {
        let s = session;
        let git = s.clone.join(".git");
        let ai = || vec![s.state.clone(), s.claude.clone()];
        let more = match role {
            Role::Lead => [vec![s.clone.clone()], ai()].concat(),
            Role::Worker | Role::Verifier => {
                [vec![s.worktree.clone(), s.target.clone(), git], ai()].concat()
            }
            Role::TestRun => vec![s.target.clone()],
        };
        let writes = Writes {
            temp: s.temp.clone(),
            more,
        };
        let mut reads: Vec<PathBuf> = SYSTEM.iter().map(PathBuf::from).collect();
        reads.extend(s.tools.iter().cloned());
        if role == Role::TestRun {
            reads.extend([s.worktree.clone(), s.clone.join(".git")]);
        }
        let network = match role {
            Role::TestRun => Network::Loopback,
            _ => Network::Out {
                server: s.server.clone(),
            },
        };
        let profile = Self {
            role,
            writes,
            reads,
            network,
        };
        profile.check(s)?;
        Ok(profile)
    }

    fn check(&self, s: &Session) -> Result<(), Refused> {
        plain(&s.home)?;
        plain(&s.runtime)?;
        let secrets = s.secrets();
        for p in self
            .writes
            .paths()
            .chain(self.reads.iter().map(PathBuf::as_path))
        {
            plain(p)?;
            if s.home.starts_with(p) {
                return Err(Refused::Home(p.to_owned()));
            }
            if secrets.iter().any(|x| x.starts_with(p) || p.starts_with(x)) {
                return Err(Refused::Secret(p.to_owned()));
            }
        }
        Ok(())
    }

    /// The role of the profile.
    pub fn role(&self) -> Role {
        self.role
    }

    /// The paths that the role writes (and reads).
    pub fn write_paths(&self) -> impl Iterator<Item = &Path> {
        self.writes.paths()
    }

    /// The paths that the role only reads.
    pub fn read_paths(&self) -> impl Iterator<Item = &Path> {
        self.reads.iter().map(PathBuf::as_path)
    }

    /// The network of the role.
    pub fn network(&self) -> &Network {
        &self.network
    }

    /// The rights of the role on the forge.
    pub fn rights(&self) -> &'static [Right] {
        self.role.rights()
    }

    /// True when the role has the right `right` on the forge.
    pub fn may(&self, right: Right) -> bool {
        self.rights().contains(&right)
    }

    /// True when the role may write `path`: it is in a write path.
    pub fn writes(&self, path: &Path) -> bool {
        self.write_paths().any(|p| path.starts_with(p))
    }

    /// True when the role may read `path`: it is in a write path or in a
    /// read path.
    pub fn reads(&self, path: &Path) -> bool {
        self.writes(path) || self.read_paths().any(|p| path.starts_with(p))
    }
}

/// The variables of the parent that reach `claude` in a session
/// (01M4BYVSR06B9HNX4SP83SY2SX). riff starts `claude` with an empty
/// environment, and then sets only these and the variables of riff
/// itself, so no credential of the person reaches a session
/// (01M4BYVSNQ5SY2GRGT73FV0Z3E). A name that ends in `_` is a prefix.
///
/// - The account and the shell: `HOME`, `USER`, `LOGNAME`, `SHELL`,
///   `PATH`.
/// - The language and the terminal: `LANG`, `LANGUAGE`, `LC_`, `TZ`,
///   `TERM`, `COLORTERM`, `NO_COLOR`, `CLICOLOR_FORCE`.
/// - tmux, for the pane of the worker: `TMUX`, `TMUX_PANE`.
/// - The folders of the person: `XDG_CONFIG_HOME`, `XDG_DATA_HOME`,
///   `XDG_STATE_HOME`, `XDG_CACHE_HOME`.
/// - The runtime folder and the session bus of the person:
///   `XDG_RUNTIME_DIR`, `DBUS_SESSION_BUS_ADDRESS`. No process in the
///   session calls systemd: only the wrapper, outside the sandbox,
///   does (01M4C2PY1DRWVWENJBM42D60M9). The `riff` of the session
///   needs them for its local folder `$XDG_RUNTIME_DIR/riff`
///   ([`crate::local`]) and for its keyring, until the secrets come
///   in the environment (#611).
/// - The network: the proxy and the certificates.
/// - Claude Code: `CLAUDE_CONFIG_DIR`. No `ANTHROPIC_` variable: a
///   session gets its model access only from the plan sign-in of the
///   person, so its use counts on the plan, never on API billing
///   (01M4C4WW8JS7QVC0ZYHWPSMWKN).
/// - riff: the server, the session, the person and the machine.
pub const KEPT_VARS: &[&str] = &[
    "HOME",
    "USER",
    "LOGNAME",
    "SHELL",
    "PATH",
    "LANG",
    "LANGUAGE",
    "LC_",
    "TZ",
    "TERM",
    "COLORTERM",
    "NO_COLOR",
    "CLICOLOR_FORCE",
    "TMUX",
    "TMUX_PANE",
    "XDG_CONFIG_HOME",
    "XDG_DATA_HOME",
    "XDG_STATE_HOME",
    "XDG_CACHE_HOME",
    "XDG_RUNTIME_DIR",
    "DBUS_SESSION_BUS_ADDRESS",
    "HTTP_PROXY",
    "HTTPS_PROXY",
    "NO_PROXY",
    "http_proxy",
    "https_proxy",
    "no_proxy",
    "SSL_CERT_FILE",
    "SSL_CERT_DIR",
    "NODE_EXTRA_CA_CERTS",
    "CLAUDE_CONFIG_DIR",
    "RIFF_SERVER",
    "RIFF_SESSION",
    "RIFF_HOME",
    "RIFF_USER",
    "RIFF_HOST",
    "RIFF_ON",
    "RIFF_MACHINE",
    "RIFF_WORKER_SLICE",
];

/// True when the variable `name` of the parent reaches `claude`
/// ([`KEPT_VARS`]).
///
/// ```
/// use riff::profile::kept;
///
/// assert!(kept("PATH"));
/// assert!(kept("LC_ALL"));
/// assert!(kept("RIFF_SESSION"));
/// assert!(!kept("GH_TOKEN"));
/// assert!(!kept("GITHUB_TOKEN"));
/// assert!(!kept("SSH_AUTH_SOCK"));
/// assert!(!kept("GIT_CONFIG_COUNT"));
/// assert!(!kept("RIFF_TEST_MARKER"));
/// assert!(!kept("LC"), "a prefix names no variable by itself");
/// assert!(!kept("ANTHROPIC_API_KEY"), "no API billing");
/// assert!(!kept("ANTHROPIC_AUTH_TOKEN"));
/// assert!(!kept("ANTHROPIC_BASE_URL"));
/// ```
pub fn kept(name: &str) -> bool {
    KEPT_VARS.iter().any(|k| match k.strip_suffix('_') {
        Some(_) => name.len() > k.len() && name.starts_with(k),
        None => name == *k,
    })
}

/// Refuses a path that is not absolute or that has a `..` component:
/// [`Path::starts_with`] compares components and does not resolve
/// them. A symlink is the same gap: the apply step (#607) resolves each
/// path on the disk before it grants it.
fn plain(p: &Path) -> Result<(), Refused> {
    if !p.is_absolute() {
        return Err(Refused::Relative(p.to_owned()));
    }
    if p.components().any(|c| c == std::path::Component::ParentDir) {
        return Err(Refused::Up(p.to_owned()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session() -> Session {
        Session {
            home: "/home/ada".into(),
            runtime: "/run/user/1000".into(),
            clone: "/home/ada/src/app".into(),
            worktree: "/home/ada/src/app/.claude/worktrees/issue-12".into(),
            target: "/home/ada/src/app/.claude/worktrees/issue-12/target".into(),
            temp: "/home/ada/.cache/riff/tmp/s1".into(),
            claude: "/home/ada/.claude".into(),
            state: "/run/user/1000/riff".into(),
            tools: vec!["/home/ada/.cargo".into(), "/home/ada/.rustup".into()],
            server: Endpoint::of_url("https://riff.example.com").unwrap(),
        }
    }

    fn writes(role: Role) -> Vec<PathBuf> {
        Profile::of(role, &session())
            .unwrap()
            .write_paths()
            .map(Path::to_path_buf)
            .collect()
    }

    fn paths(list: &[&str]) -> Vec<PathBuf> {
        list.iter().map(PathBuf::from).collect()
    }

    #[test]
    fn each_role_writes_the_paths_of_its_row() {
        let wt = "/home/ada/src/app/.claude/worktrees/issue-12";
        let target = "/home/ada/src/app/.claude/worktrees/issue-12/target";
        let (temp, git) = ("/home/ada/.cache/riff/tmp/s1", "/home/ada/src/app/.git");
        let (state, claude) = ("/run/user/1000/riff", "/home/ada/.claude");
        assert_eq!(
            writes(Role::Lead),
            paths(&[temp, "/home/ada/src/app", state, claude])
        );
        let agent = paths(&[temp, wt, target, git, state, claude]);
        assert_eq!(writes(Role::Worker), agent);
        assert_eq!(writes(Role::Verifier), agent);
        assert_eq!(writes(Role::TestRun), paths(&[temp, target]));
    }

    #[test]
    fn a_worker_writes_its_worktree_and_the_git_dir_not_the_clone() {
        let w = Profile::of(Role::Worker, &session()).unwrap();
        assert!(w.writes(Path::new("/home/ada/src/app/.git/refs/heads/x")));
        assert!(w.writes(Path::new(
            "/home/ada/src/app/.claude/worktrees/issue-12/Cargo.toml"
        )));
        assert!(!w.writes(Path::new("/home/ada/src/app/Cargo.toml")));
        assert!(!w.writes(Path::new(
            "/home/ada/src/app/.claude/worktrees/issue-7/Cargo.toml"
        )));
        let lead = Profile::of(Role::Lead, &session()).unwrap();
        assert!(lead.writes(Path::new("/home/ada/src/app/Cargo.toml")));
    }

    #[test]
    fn only_the_test_run_reads_the_worktree_with_no_write() {
        let t = Profile::of(Role::TestRun, &session()).unwrap();
        let src = Path::new("/home/ada/src/app/.claude/worktrees/issue-12/src/lib.rs");
        assert!(t.reads(src) && !t.writes(src));
        let git = Path::new("/home/ada/src/app/.git/objects/ab");
        assert!(t.reads(git) && !t.writes(git));
        assert!(t.reads(Path::new("/usr/bin/git")));
        assert!(t.reads(Path::new("/home/ada/.cargo/bin/cargo")));
    }

    #[test]
    fn each_role_has_the_ports_of_its_row() {
        for role in [Role::Lead, Role::Worker, Role::Verifier] {
            let p = Profile::of(role, &session()).unwrap();
            assert_eq!(p.network().ports(), [443], "{role}");
            let hosts: Vec<String> = p
                .network()
                .endpoints()
                .into_iter()
                .map(|e| e.host)
                .collect();
            for host in ["riff.example.com", "github.com", "index.crates.io", MODEL] {
                assert!(hosts.iter().any(|h| h == host), "{role}: {host}");
            }
        }
        let t = Profile::of(Role::TestRun, &session()).unwrap();
        assert_eq!(t.network(), &Network::Loopback);
        assert!(t.network().ports().is_empty());
    }

    #[test]
    fn each_role_has_the_forge_rights_of_its_row() {
        use Right::*;
        assert_eq!(
            Role::Lead.rights(),
            [Read, Plan, Comment, Push, PullRequest]
        );
        assert_eq!(Role::Worker.rights(), [Read, Comment, Push, PullRequest]);
        assert_eq!(Role::Verifier.rights(), [Read, Comment, Verify]);
        assert!(Role::TestRun.rights().is_empty());
        assert!(!Role::Worker.rights().contains(&Plan));
        assert!(!Role::Lead.rights().contains(&Verify));
    }

    #[test]
    fn no_role_reaches_the_home_the_keyring_or_the_bus_of_the_person() {
        let s = session();
        let places = [
            s.home.clone(),
            s.home.join(".bashrc"),
            s.home.join(".local/share/keyrings/login.keyring"),
            s.home.join(".ssh/id_ed25519"),
            s.home.join(".config/gh/hosts.yml"),
            s.runtime.join("bus"),
            s.runtime.join("keyring/control"),
        ];
        for role in Role::ALL {
            let p = Profile::of(role, &s).unwrap();
            for place in &places {
                assert!(!p.reads(place), "{role} reads {}", place.display());
                assert!(!p.writes(place), "{role} writes {}", place.display());
            }
        }
    }

    /// 01M4C2PY1DRWVWENJBM42D60M9: no process of a sandbox calls
    /// systemd. The worker profile reaches no socket of systemd: not
    /// the user bus, not the private socket of the user manager, not
    /// the system bus. The same is true of each other role.
    #[test]
    fn the_worker_profile_has_no_access_to_the_systemd_user_bus() {
        let s = session();
        let sockets = [
            s.runtime.join("bus"),
            s.runtime.join("systemd/private"),
            s.runtime.join("systemd/notify"),
            PathBuf::from("/run/dbus/system_bus_socket"),
        ];
        for role in Role::ALL {
            let p = Profile::of(role, &s).unwrap();
            for socket in &sockets {
                assert!(!p.reads(socket), "{role} reads {}", socket.display());
                assert!(!p.writes(socket), "{role} writes {}", socket.display());
            }
        }
        // A session whose riff state is the folder of the user manager
        // makes no profile.
        let bad = Session {
            state: s.runtime.join("systemd"),
            ..s
        };
        assert!(matches!(
            Profile::of(Role::Worker, &bad),
            Err(Refused::Secret(_))
        ));
    }

    #[test]
    fn a_session_that_gives_the_home_or_a_secret_makes_no_profile() {
        let home = Session {
            clone: "/home/ada".into(),
            ..session()
        };
        assert_eq!(
            Profile::of(Role::Lead, &home),
            Err(Refused::Home("/home/ada".into()))
        );
        let root = Session {
            tools: vec!["/".into()],
            ..session()
        };
        for role in Role::ALL {
            assert_eq!(Profile::of(role, &root), Err(Refused::Home("/".into())));
        }
        let runtime = Session {
            state: "/run/user/1000".into(),
            ..session()
        };
        assert_eq!(
            Profile::of(Role::Worker, &runtime),
            Err(Refused::Secret("/run/user/1000".into()))
        );
        let gh = Session {
            tools: vec!["/home/ada/.config".into()],
            ..session()
        };
        assert_eq!(
            Profile::of(Role::TestRun, &gh),
            Err(Refused::Secret("/home/ada/.config".into()))
        );
        let relative = Session {
            temp: "tmp".into(),
            ..session()
        };
        assert_eq!(
            Profile::of(Role::TestRun, &relative),
            Err(Refused::Relative("tmp".into()))
        );
    }

    #[test]
    fn a_path_with_a_parent_component_makes_no_profile() {
        let up = Session {
            clone: "/home/ada/src/..".into(),
            ..session()
        };
        assert_eq!(
            Profile::of(Role::Lead, &up),
            Err(Refused::Up("/home/ada/src/..".into()))
        );
        let ssh = Session {
            tools: vec!["/home/ada/src/../.ssh".into()],
            ..session()
        };
        for role in Role::ALL {
            assert_eq!(
                Profile::of(role, &ssh),
                Err(Refused::Up("/home/ada/src/../.ssh".into()))
            );
        }
        let home = Session {
            home: "/home/ada/x/..".into(),
            ..session()
        };
        assert_eq!(
            Profile::of(Role::Worker, &home),
            Err(Refused::Up("/home/ada/x/..".into()))
        );
    }
}
