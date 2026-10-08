//! The sandbox of a session: riff applies the profile of its role with
//! Landlock before it starts `claude`.
//!
//! # Design
//!
//! `riff workers run` starts `claude` through `riff workers sandbox`
//! (01M4BTB72DY0C5Y0EJVR0ZH6FZ). The sandbox is the last step before
//! `claude`: `systemd-run` and `nice` come first, so they still reach
//! the user manager of systemd. `riff workers sandbox` builds the
//! [`Session`] of the worker from its environment ([`Here`]), makes the
//! [`Profile`] of the role, restricts itself with Landlock, and then
//! runs `claude` in its place. Each child of `claude` (the Bash
//! commands, `cargo`, the tests) inherits the sandbox. No child can
//! remove it or make it wider. `riff workers lead` starts the lead the
//! same way, with `--role lead` (01M4DDWP9XSA14E0YF211XZYKR). Its steps
//! in tmux and on the processes of its workers go through its broker
//! ([`crate::door`]).
//!
//! | Landlock limits | From |
//! |---|---|
//! | The files that a session reads and writes | [`Profile::write_paths`], [`Profile::read_paths`], [`DEVICES`] |
//! | The TCP ports that a session connects to | [`Network::ports`](crate::profile::Network::ports) and [`local_ports`] |
//! | A signal to a process outside the sandbox | the scope of signals (ABI 6) |
//! | A connect to an abstract unix socket outside the sandbox | the scope of abstract unix sockets (ABI 6) |
//!
//! Landlock also stops a process of the sandbox from a trace of a
//! process outside it. So a session cannot read `/proc/PID/environ` of
//! another process of the person.
//!
//! ```mermaid
//! flowchart LR
//!     W["riff workers run"] --> S["systemd-run --scope, nice"]
//!     S --> X["riff workers sandbox"]
//!     X -->|"Here: the environment"| P["resolve each path, Profile::of"]
//!     P --> L["Landlock: restrict self"]
//!     L -->|"exec"| C["claude, and each child"]
//! ```
//!
//! - **Best effort** (01M4BTB74RD24WH59FA65KZ8J0). riff asks for each
//!   right of [`ABI::V9`], and the kernel applies the rights of its own
//!   ABI. A kernel with no Landlock gives one error line, and no
//!   session starts: never a session with no sandbox.
//! - **Each path is resolved** (01M4BR61PPQV7JJE5Y2G9Q90AF,
//!   01M4BTB7757XF8RZ6MRXKTM8SB). [`resolve`] follows each symlink before
//!   [`Profile::of`] checks the path. So a worktree that is a link to
//!   the home of the person gives no profile.
//! - **The ports** (01M4BTB79FZ5RAFXMZRRJZNCH9). A session connects to
//!   the ports of its profile, and to the local port range of the
//!   kernel: a test server on port 0 gets a port in that range. A
//!   sandbox cannot be made wider, so a test run in a worker keeps this
//!   rule until the test run has its own network (#612).
//! - **A Claude folder for each session** (01M4BTB7BPA38ARM789WBV507D).
//!   The Claude folder of the person (`~/.claude`) holds its settings,
//!   its sign-in and the memory of each project. No session writes it.
//!   Each session gets its own Claude folder ([`claude_dir`]) in
//!   `CLAUDE_CONFIG_DIR`.
//! - **The permission rules** (01M4BTB7DY1Y74PP3JWKVX58JQ). The file of
//!   the permission rules of a session ([`rules_file`]) is outside each
//!   write path of its profile. So no session makes its own rules
//!   wider (#613).
//! - **The devices** (01M4BTB7MSEWG4XY1CRRNBNRPH). Each session writes
//!   the devices of [`DEVICES`], for example `/dev/null` and its
//!   terminal.
//!
//! # Threat model
//!
//! The book has this part for people: "The threat model of the
//! sandbox" in `how-it-works.md`. The rule is 01M4DDZ8B3QBZATBPNHVFDH7BR.
//!
//! riff trusts you and the code of riff. It does not trust an AI
//! session, or a test that a session writes. A session can try each
//! thing that its sandbox allows.
//!
//! | Actor | Holds | Runs |
//! |---|---|---|
//! | You | your home, your keyring, your SSH keys, the sign-in of `gh`, your Claude plan token | outside each sandbox |
//! | The lead | its grant, the forge token of the lead | in its sandbox; its broker runs its tmux steps |
//! | A worker or a verifier | its grant, the forge token of its role, its worktree | in its sandbox |
//! | A test run | its temp folder and its target | in its own namespaces, with loopback only |
//! | riff outside: the wrapper, the broker, `riff worktrees clean`, `riff workers host`, the start of `riff` | your rights | outside each sandbox, as you |
//! | riff-server | the facts of each session, the key of the GitHub App | on the shared server |
//! | GitHub | the repository, the ruleset of `main` | on GitHub |
//!
//! ```mermaid
//! flowchart LR
//!     subgraph M["your machine"]
//!         O["you and riff outside"]
//!         subgraph S["sandbox of a session"]
//!             A["claude and each child"]
//!             subgraph T["test run"]
//!                 X["each test"]
//!             end
//!         end
//!     end
//!     A -->|"broker request"| O
//!     A -->|"files it writes"| O
//!     A -->|"grant, token request"| R["riff-server"]
//!     R -->|"forge token"| A
//!     A -->|"push, pull request"| G["GitHub"]
//! ```
//!
//! The arrows that go out of a sandbox are the shared surfaces. riff
//! keeps one rule on each of them:
//!
//! - Nothing that a session writes is run, or read as config, by a
//!   process outside a sandbox, with no check.
//! - A trusted service (the broker, riff-server) takes no path, role or
//!   repository from a request. It takes them from its own facts.
//!
//! ## The shared surfaces
//!
//! Each row is a surface with a control, and the test of that control.
//! The book has the same rows, and `hygiene::surfaces` checks the two
//! tables and each test (01M4DDZ8DFY1SP1AVVRSD0DRV4). Add a row for
//! each new thing that a session writes or asks and that riff reads
//! outside a sandbox.
//!
//! <!-- surfaces -->
//! | Surface | A session writes or asks | Read or run outside by | Control | Test |
//! |---|---|---|---|---|
//! | The git dir of the clone | a worker: its objects, refs, logs and worktrees | each git command of riff in the clone | a worker writes no config and no hooks; riff runs git with no hooks, no fsmonitor and no submodule | `a_worker_does_only_what_its_profile_allows`, `a_worker_commits_and_pushes_with_no_write_of_the_git_config` |
//! | The git dirs of a worktree | the `.git` file of its worktree, `gitdir` and `commondir` in `.git/worktrees/NAME` | `riff worktrees clean` | riff sets the git dirs itself, and runs no git in a worktree that names another git dir | `worktrees_clean_runs_no_program_of_a_git_dir_that_a_session_names`, `a_link_or_a_way_out_of_the_worktree_gives_no_git`, `a_clone_with_a_config_of_each_worktree_gives_no_git` |
//! | The files of a worktree | each file, for example `.gitattributes` | `git status`, `add` and `commit` of riff | these git steps run in the sandbox of a worker | `worktrees_clean_reads_a_worktree_with_the_rights_of_a_worker` |
//! | A broker request | an operation, a folder and variables | `riff workers broker` | only the operations of its list, a folder in the worktree, only the variables of cargo and the tests | `an_unknown_operation_is_refused`, `the_broker_keeps_only_the_variables_of_the_tests`, `a_test_run_in_a_worker_runs_through_the_broker` |
//! | The folders of a test run | a target, a planted `.git` file | `riff test-run` | the target, the clone and the worktree come from the broker | `a_test_run_writes_no_folder_that_the_request_names`, `a_planted_git_file_gives_a_test_run_no_other_git_dir` |
//! | The environment of a test run | the variables of the request | each test | an empty environment, then only `TEST_RUN_VARS` | `no_credential_and_no_unknown_variable_of_the_parent_reaches_a_test` |
//! | The cargo home | a read of the registry token | `cargo` of the person | no role reads `credentials.toml` or `credentials` | `no_session_and_no_test_run_reads_the_cargo_registry_tokens` |
//! | A forge token request | a call as its session | riff-server | the role and the repository come from the facts of the server; a new claim revokes the old token | `the_server_takes_the_repository_from_its_facts_not_from_the_call`, `each_role_gets_its_rights_on_the_repository_of_its_session_only`, `a_change_of_claim_revokes_the_old_token` |
//! | The session grant | a call with its grant | riff-server | the grant acts only as its session; only the session key ends it | `a_grant_acts_only_as_its_session_and_lives_through_a_restart`, `only_the_session_key_ends_a_grant_and_its_tokens`, `a_session_works_with_only_its_environment_and_leaks_no_secret` |
//! | The permission rules of a session | a write of its rules file | `claude` | the rules file is outside each write path | `no_role_writes_the_permission_rules_of_its_session`, `a_worker_does_only_what_its_profile_allows` |
//! | The plugin of riff and the git config of the person | a write | `claude`, git | a session reads them only | `a_worker_does_only_what_its_profile_allows` |
//! | The Claude folder of the person | a write of its settings | the `claude` of the person | no session writes it; each session has its own Claude folder | `a_worker_does_only_what_its_profile_allows`, `no_role_edits_a_settings_file_of_claude_code` |
//! | The tmux servers and the D-Bus | a connect | tmux, D-Bus services | no unix socket with a name; no `TMUX`; the bus is a secret path | `a_worker_reaches_no_tmux_server`, `no_role_reaches_the_home_the_keyring_or_the_bus_of_the_person` |
//! | The other processes of the person | a signal, a read of `/proc/PID/environ` | each process | the scope of signals of Landlock; no trace | `a_worker_does_only_what_its_profile_allows` |
//! | The user manager of systemd | a call | systemd | no profile reaches a bus of systemd | `the_worker_profile_has_no_access_to_the_systemd_user_bus` |
//! | A host request | a request in the name of the lead | `riff workers host` | only a signed request of the lead | `a_host_refuses_a_request_that_is_not_from_the_lead` |
//! | The stop file | `.riff-stop` in its temp folder | the wrapper | the wrapper reads only that the file is there | `the_server_stops_an_idle_worker_through_its_wrapper` |
//! | The lead | the worktrees of the clone and the git parts of a worker; an operation of its broker | each git command of riff, tmux, the processes of its workers | the lead writes no config, hooks, info, packed-refs or other file of the clone; its tmux steps and signals are operations of its broker, only on the workers with the clone mark of its clone | `a_lead_in_its_sandbox_starts_and_stops_a_worker_through_the_broker`, `the_lead_writes_no_config_hooks_info_or_packed_refs_of_the_clone`, `a_stop_of_a_worker_of_another_clone_stops_nothing`, `only_the_broker_of_a_lead_runs_the_operations_of_the_lead` |
//! | The MCP config of a session | a write of `workers-mcp.json` | the `claude` of the lead and of each worker | the file is in the given folder of riff, outside each write path | `a_lead_in_its_sandbox_starts_and_stops_a_worker_through_the_broker` |
//! | The refs of the clone | a worker: its refs, also `refs/remotes/origin/HEAD`, and the `HEAD` of its worktree | the fast-forward of the main clone, `riff worktrees clean`, the rules of riff | riff asks `origin` for the default branch and names each ref in full; the branch of a worktree comes from its name; riff refuses a branch name that starts with `-`, and puts `--` before each name | `the_fast_forward_takes_the_default_branch_from_origin_not_from_a_planted_ref`, `the_default_branch_comes_from_origin_not_from_a_planted_ref`, `worktrees_clean_takes_the_branch_from_the_name_not_from_the_head`, `worktrees_clean_puts_two_dashes_before_each_name` |
//! <!-- /surfaces -->
//!
//! ## The shared surfaces with no control yet
//!
//! Each row is a surface with no control yet. It names its issue, or a
//! proposed accept. Mike signs off this table before the release 2.0.0.
//!
//! <!-- open-surfaces -->
//! | Surface | A session writes or asks | Read or run outside by | Risk | Decision |
//! |---|---|---|---|---|
//! | The tmux config of riff | `tmux.conf` in the riff state folder | the tmux server of riff, at its start | a program runs outside a sandbox, a file of the person changes | #655: merge before 2.0.0, or Mike accepts at the sign-off |
//! | The list of clones | `clones` in the riff state folder | `riff`, at its start | riff starts the lead in a folder that a session picked | #655: merge before 2.0.0, or Mike accepts at the sign-off |
//! | The deaths of workers | `worker-deaths` in the riff state folder | the rollout, each workers host | riff starts or stops workers on a false count, a file of the person changes | #655: merge before 2.0.0, or Mike accepts at the sign-off |
//! | The context files of a worker | the start time of a context in the riff state folder | `riff workers reap` | the reap stops the wrong processes of a worker | #655: merge before 2.0.0, or Mike accepts at the sign-off |
//! | The pool of build jobs | `jobs` in the riff state folder | the wrapper of each worker | a session takes the build jobs of other workers, a file of the person changes | #655: merge before 2.0.0, or Mike accepts at the sign-off |
//! | The said-once files and the update files | `no-jobserver`, `no-systemd` and the like, `update.lock`, `update-tried`, `update.log` | riff, the update of riff | riff says a thing one time too few, an update waits, a file of the person changes | Accept (proposed): they hold no command; #655 makes each write of riff there follow no link |
//! | The locks and the compact record | `workers-limit.lock`, `clear-ID.lock`, the compact lock and record | the hooks and checks of riff | a step of riff waits | Accept (proposed): a lock or a record holds no command |
//! | The forge token files | its temp folder | the wrapper | a file of the person changes | #655: merge before 2.0.0, or Mike accepts at the sign-off |
//! | The folder of a broker request | a folder | `riff workers broker` | the broker runs in another folder than the one it checked | #614, #654 |
//! | The variables of a broker request | a bus address | `riff test-run` | a test run gets a variable that is not of cargo or the tests | #654 |
//! | The worktrees of other sessions | the worktrees folder of the clone | the other sessions | a worker changes the work of another session | #645 |
//! | The clear of a worker | a clear of its context | tmux | a worker in its sandbox cannot clear its context | #653 |
//! | Forge tokens in an allowed account | a session in a repository of the account | riff-server | each member of the riff gets a token for each repository of an allowed account where it has a session | Accept (proposed): the owner admits each member, and the token has the rights of the role only |
//! | The lead token of a person | a token with no session | riff-server | the token lives up to one hour after the allow or the lead ends | Accept (proposed): one hour at most |
//! | A session grant with no process | none | riff-server | the grant stays 7 days when `claude` does not start | Accept (proposed): only the session key uses it, and the key was only in the wrapper |
//! <!-- /open-surfaces -->
//!
//! # Gaps
//!
//! Landlock limits a connect to a unix socket with a path (for example
//! the D-Bus socket of the person) only from ABI 9 (Linux 7.1). On
//! Linux 7.0 (ABI 8), the seccomp filter of [`apply`] stops it: no session
//! makes a unix socket with a name. Each other gap is a row of the
//! table of the shared surfaces with no control yet.
//!
//! ```no_run
//! use riff::profile::Role;
//! use riff::confine::{self, Here};
//!
//! let here = Here::of_process("https://riff.example.com")?;
//! let profile = confine::profile(Role::Worker, &here)?;
//! let applied = confine::apply(&profile, &confine::ports(&profile))?;
//! println!("{applied}");
//! // Now run claude in the place of this process.
//! # Ok::<(), anyhow::Error>(())
//! ```

use std::ffi::OsString;
use std::fmt;
use std::ops::RangeInclusive;
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use landlock::{
    ABI, Access, AccessFs, AccessNet, LandlockStatus, NetPort, PathBeneath, PathFd, Ruleset,
    RulesetAttr, RulesetCreatedAttr, RulesetStatus, Scope,
};

use crate::profile::{Endpoint, Profile, Role, Session};

/// The highest Landlock ABI that riff asks for. The kernel applies the
/// rights of its own ABI (01M4BTB74RD24WH59FA65KZ8J0).
pub const ABI_WANTED: ABI = ABI::V9;

/// The device files that each session writes (01M4BTB7MSEWG4XY1CRRNBNRPH).
/// `/dev/pts` holds the terminals.
pub const DEVICES: [&str; 8] = [
    "/dev/null",
    "/dev/zero",
    "/dev/full",
    "/dev/random",
    "/dev/urandom",
    "/dev/tty",
    "/dev/ptmx",
    "/dev/pts",
];

/// The local port range when the kernel does not say it: the default
/// of Linux.
pub const LOCAL_PORTS: RangeInclusive<u16> = 32768..=60999;

/// The variable of Claude Code that names its Claude folder.
pub const CLAUDE_CONFIG_DIR: &str = "CLAUDE_CONFIG_DIR";

/// The local port range of the kernel, from
/// `/proc/sys/net/ipv4/ip_local_port_range`, else [`LOCAL_PORTS`].
pub fn local_ports() -> RangeInclusive<u16> {
    std::fs::read_to_string("/proc/sys/net/ipv4/ip_local_port_range")
        .ok()
        .and_then(|s| port_range(&s))
        .unwrap_or(LOCAL_PORTS)
}

/// The port range of the text of `ip_local_port_range`.
///
/// ```
/// use riff::confine::port_range;
///
/// assert_eq!(port_range("32768\t60999\n"), Some(32768..=60999));
/// assert_eq!(port_range("x"), None);
/// assert_eq!(port_range("9 3"), None);
/// ```
pub fn port_range(text: &str) -> Option<RangeInclusive<u16>> {
    let mut parts = text.split_whitespace().map(str::parse::<u16>);
    let (low, high) = (parts.next()?.ok()?, parts.next()?.ok()?);
    (low <= high).then_some(low..=high)
}

/// The TCP ports that a session of `profile` connects to: the ports of
/// the profile, and the local port range (01M4BTB79FZ5RAFXMZRRJZNCH9).
/// A role with no network has none.
pub fn ports(profile: &Profile) -> Vec<u16> {
    ports_with(profile, local_ports())
}

/// [`ports`] with the local port range `local`.
///
/// ```
/// use riff::profile::{Endpoint, Network};
/// # use riff::profile::{Profile, Role, Session};
/// # let s = Session {
/// #     home: "/h".into(), runtime: "/run/user/7".into(), clone: "/h/app".into(),
/// #     worktree: "/h/app".into(), target: "/h/app/target".into(), temp: "/h/tmp".into(),
/// #     claude: "/h/c".into(), rules: "/h/r.json".into(), state: "/run/user/7/riff".into(),
/// #     tools: vec![], server: Endpoint::of_url("http://127.0.0.1:7878").unwrap(),
/// # };
/// let worker = Profile::of(Role::Worker, &s).unwrap();
/// assert_eq!(riff::confine::ports_with(&worker, 7000..=7001), [443, 7000, 7001, 7878]);
/// let test = Profile::of(Role::TestRun, &s).unwrap();
/// assert!(riff::confine::ports_with(&test, 7000..=7001).is_empty());
/// ```
pub fn ports_with(profile: &Profile, local: RangeInclusive<u16>) -> Vec<u16> {
    let mut ports = profile.network().ports();
    if !ports.is_empty() {
        ports.extend(local);
    }
    ports.sort_unstable();
    ports.dedup();
    ports
}

/// The root of the files that riff keeps for each session:
/// `$XDG_DATA_HOME/riff`, else `~/.local/share/riff`. It is never in
/// `RIFF_HOME`: that can be a worktree, and a worktree is a write path.
///
/// ```
/// use riff::confine::data_from;
///
/// assert_eq!(data_from(Some("/d".into()), Some("/h".into())), Some("/d/riff".into()));
/// assert_eq!(data_from(Some("".into()), Some("/h".into())), Some("/h/.local/share/riff".into()));
/// assert_eq!(data_from(None, None), None);
/// ```
pub fn data_from(data_home: Option<OsString>, home: Option<OsString>) -> Option<PathBuf> {
    let set = |v: Option<OsString>| v.filter(|v| !v.is_empty()).map(PathBuf::from);
    set(data_home)
        .map(|d| d.join("riff"))
        .or_else(|| set(home).map(|h| h.join(".local/share/riff")))
}

/// The Claude folder of `session` in the data root `data`
/// (01M4BTB7BPA38ARM789WBV507D).
///
/// ```
/// assert_eq!(
///     riff::confine::claude_dir("/d".as_ref(), "s1"),
///     std::path::Path::new("/d/claude/s1")
/// );
/// ```
pub fn claude_dir(data: &Path, session: &str) -> PathBuf {
    data.join("claude").join(session)
}

/// The folder of the files that riff gives `claude` at each start, in
/// the data root `data`: the MCP config of the lead and of each worker
/// ([`crate::worker_mcp`]). Each AI role reads it, and no role writes
/// it (01M4DDWPGRM1P4A76GFHPQHMQF): so no session changes the MCP
/// servers of the next `claude`. The plugin
/// ([`crate::plugin::dir`]) and the rules files ([`rules_file`]) are in
/// the data root too, outside each write path.
///
/// ```
/// assert_eq!(
///     riff::confine::given_dir("/d".as_ref()),
///     std::path::Path::new("/d/given")
/// );
/// ```
pub fn given_dir(data: &Path) -> PathBuf {
    data.join("given")
}

/// [`given_dir`] of this process: in `XDG_DATA_HOME`, else in `HOME`.
pub fn given_here() -> Result<PathBuf> {
    let var = |name: &str| std::env::var_os(name);
    data_from(var("XDG_DATA_HOME"), var("HOME"))
        .map(|data| given_dir(&data))
        .context("cannot find the data folder of riff: set HOME")
}

/// The file of the permission rules of `session` in the data root
/// `data`: outside each write path of a profile
/// (01M4BTB7DY1Y74PP3JWKVX58JQ).
///
/// ```
/// assert_eq!(
///     riff::confine::rules_file("/d".as_ref(), "s1"),
///     std::path::Path::new("/d/rules/s1.json")
/// );
/// ```
pub fn rules_file(data: &Path, session: &str) -> PathBuf {
    data.join("rules").join(format!("{session}.json"))
}

/// What riff knows of a session from its process: the facts that
/// [`Here::session`] makes the [`Session`] from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Here {
    /// The ID of the session (`RIFF_SESSION`).
    pub session: String,
    /// `HOME`.
    pub home: PathBuf,
    /// `XDG_RUNTIME_DIR`, else `/run/user/UID`.
    pub runtime: PathBuf,
    /// The top of the git worktree of the current dir. A worker starts
    /// in the main clone: then the profile takes the folder of the
    /// worktrees of the clone (01M4BT341H1M1N1MT947HXNXDR).
    pub worktree: PathBuf,
    /// The git dir of the main clone (`git rev-parse --git-common-dir`).
    pub common: PathBuf,
    /// `CARGO_TARGET_DIR`. With none, the target is in the worktree.
    pub target: Option<PathBuf>,
    /// The temp folder of the session (`TMPDIR`).
    pub temp: PathBuf,
    /// The data root of riff ([`data_from`]).
    pub data: PathBuf,
    /// The local files of riff ([`crate::local::dir`]).
    pub state: PathBuf,
    /// The paths that each role reads: each folder of `PATH`, the
    /// toolchain, the folders of the programs, the settings of riff and
    /// of git.
    pub tools: Vec<PathBuf>,
    /// The riff server.
    pub server: Endpoint,
}

impl Here {
    /// The facts of this process in its current dir, with the riff
    /// server `server`.
    pub fn of_process(server: &str) -> Result<Self> {
        Self::of_dir(&std::env::current_dir()?, server, None)
    }

    /// The facts of this process in the dir `cwd`, with the riff server
    /// `server`. `name` names the Claude folder and the rules file of
    /// the session; with none, riff takes `RIFF_SESSION`. The temp folder
    /// is the temp dir of this process.
    pub fn of_dir(cwd: &Path, server: &str, name: Option<&str>) -> Result<Self> {
        let var = |name: &str| std::env::var_os(name).filter(|v| !v.is_empty());
        let session = name
            .map(str::to_owned)
            .or_else(|| std::env::var(crate::identity::SESSION_VARS[0]).ok())
            .filter(|s| !s.is_empty())
            .context("the sandbox needs RIFF_SESSION or a name")?;
        let home = PathBuf::from(var("HOME").context("the sandbox needs HOME")?);
        let runtime = var("XDG_RUNTIME_DIR").map_or_else(
            || {
                use std::os::unix::fs::MetadataExt;
                let uid = std::fs::metadata("/proc/self").map_or(0, |m| m.uid());
                PathBuf::from(format!("/run/user/{uid}"))
            },
            PathBuf::from,
        );
        let worktree = git_path(cwd, "--show-toplevel")?;
        let common = git_path(cwd, "--git-common-dir")?;
        let target = var("CARGO_TARGET_DIR").map(PathBuf::from);
        let temp = std::env::temp_dir();
        let data =
            data_from(var("XDG_DATA_HOME"), var("HOME")).context("the sandbox needs HOME")?;
        let state = crate::local::dir().context("the sandbox needs a local dir of riff")?;
        let mut tools: Vec<PathBuf> = var("PATH")
            .map(|p| std::env::split_paths(&p).collect())
            .unwrap_or_default();
        let cargo = var("CARGO_HOME").map_or_else(|| home.join(".cargo"), PathBuf::from);
        let rustup = var("RUSTUP_HOME").map_or_else(|| home.join(".rustup"), PathBuf::from);
        // Not the cargo home itself: it holds the registry tokens
        // (01M4D4BZ41AH29KSA9B0VZB8DQ).
        tools.extend(crate::profile::cargo_reads(&cargo));
        tools.extend([rustup, home.join(".gitconfig"), home.join(".config/git")]);
        tools.extend(crate::plugin::dir().ok());
        tools.push(given_dir(&data));
        // `claude` on the PATH is a link to the folder of its version.
        tools.extend(
            on_path(Path::new("claude"))
                .map(|p| resolve(&p))
                .and_then(|p| p.parent().map(Path::to_path_buf)),
        );
        // `/etc/resolv.conf` is often a link into `/run`: with no read of
        // it, a session finds no host (01M4C5RTX728DS9FHF7HE14G6T).
        tools.push(resolve(Path::new("/etc/resolv.conf")));
        if let Some(settings) = crate::settings::path()
            .ok()
            .and_then(|p| p.parent().map(Path::to_path_buf))
        {
            tools.push(settings);
        }
        if let Ok(riff) = crate::binary::this_on_disk()
            && let Some(dir) = riff
                .canonicalize()
                .ok()
                .and_then(|p| p.parent().map(Path::to_path_buf))
        {
            tools.push(dir);
        }
        let server = Endpoint::of_url(server)
            .with_context(|| format!("the sandbox cannot read the server URL {server}"))?;
        Ok(Self {
            session,
            home,
            runtime,
            worktree,
            common,
            target,
            temp,
            data,
            state,
            tools,
            server,
        })
    }

    /// The same facts, with the folder of the program `program` in the
    /// tools: the program and the files next to it.
    pub fn with_program(mut self, program: &Path) -> Self {
        let real = on_path(program).map(|p| resolve(&p));
        if let Some(dir) = real.as_deref().and_then(Path::parent) {
            self.tools.push(dir.to_path_buf());
        }
        self
    }

    /// The same facts, with the temp folder `temp`: the wrapper of a
    /// worker makes it before `claude` gets it in `TMPDIR`.
    pub fn with_temp(mut self, temp: &Path) -> Self {
        self.temp = temp.to_path_buf();
        self
    }

    /// The [`Session`] of these facts, with each path resolved
    /// ([`resolve`]). A tool path that gives the home of the person or a
    /// secret is left out (01M4BTB7757XF8RZ6MRXKTM8SB). Each other path
    /// goes as it is: [`Profile::of`] checks it.
    ///
    /// ```
    /// use riff::profile::Endpoint;
    /// use riff::confine::Here;
    ///
    /// let here = Here {
    ///     session: "s1".into(),
    ///     home: "/nowhere/h".into(),
    ///     runtime: "/nowhere/run".into(),
    ///     worktree: "/nowhere/h/app".into(),
    ///     common: "/nowhere/h/app/.git".into(),
    ///     target: None,
    ///     temp: "/nowhere/tmp/s1".into(),
    ///     data: "/nowhere/h/.local/share/riff".into(),
    ///     state: "/nowhere/run/riff".into(),
    ///     tools: vec!["/usr/bin".into(), "/nowhere/h".into(), "/nowhere/h/.ssh".into()],
    ///     server: Endpoint::of_url("http://127.0.0.1:7878").unwrap(),
    /// };
    /// let s = here.session();
    /// assert_eq!(s.clone, std::path::Path::new("/nowhere/h/app"));
    /// // A worker starts in the main clone: it gets the folder of the
    /// // worktrees.
    /// assert_eq!(s.worktree, std::path::Path::new("/nowhere/h/app/.claude/worktrees"));
    /// assert_eq!(s.target, s.worktree);
    /// assert_eq!(s.claude, std::path::Path::new("/nowhere/h/.local/share/riff/claude/s1"));
    /// // The tools keep `/usr/bin` and the cargo settings of the clone;
    /// // the home and a secret go.
    /// assert_eq!(s.tools, ["/usr/bin", "/nowhere/h/app/.cargo"].map(std::path::PathBuf::from));
    /// ```
    pub fn session(&self) -> Session {
        let common = resolve(&self.common);
        let clone = match common.file_name() {
            Some(name) if name == ".git" => {
                common.parent().map_or(common.clone(), Path::to_path_buf)
            }
            _ => common,
        };
        let worktrees = clone.join(".claude/worktrees");
        let mut worktree = resolve(&self.worktree);
        if worktree == clone {
            worktree = worktrees.clone();
        }
        let target = match &self.target {
            Some(target) => resolve(target),
            None if worktree == worktrees => worktree.clone(),
            None => worktree.join("target"),
        };
        let mut session = Session {
            home: resolve(&self.home),
            runtime: resolve(&self.runtime),
            clone,
            worktree,
            target,
            temp: resolve(&self.temp),
            claude: resolve(&claude_dir(&self.data, &self.session)),
            rules: resolve(&rules_file(&self.data, &self.session)),
            state: resolve(&self.state),
            tools: vec![],
            server: self.server.clone(),
        };
        let mut tools: Vec<PathBuf> = vec![];
        // A worktree is in the clone, so cargo reads the cargo settings of
        // the clone (01M4C5RTX728DS9FHF7HE14G6T).
        let cargo = session.clone.join(".cargo");
        for tool in self.tools.iter().map(|t| resolve(t)).chain([cargo]) {
            if session.refuses(&tool).is_none() && !tools.contains(&tool) {
                tools.push(tool);
            }
        }
        session.tools = tools;
        session
    }
}

/// The path of `program`: itself when it names a folder, else the first
/// match in `PATH`.
fn on_path(program: &Path) -> Option<PathBuf> {
    if program.components().count() > 1 {
        return Some(program.to_path_buf());
    }
    std::env::split_paths(&std::env::var_os("PATH")?)
        .map(|dir| dir.join(program))
        .find(|p| p.is_file())
}

/// The path of `git rev-parse --path-format=absolute ARG` in `dir`.
fn git_path(dir: &Path, arg: &str) -> Result<PathBuf> {
    let out = git_in(dir)?
        .args(["rev-parse", "--path-format=absolute", arg])
        .output()
        .context("cannot run git")?;
    if !out.status.success() {
        bail!(
            "the sandbox needs a git worktree: {} is not in one",
            dir.display()
        );
    }
    Ok(PathBuf::from(String::from_utf8_lossy(&out.stdout).trim()))
}

/// `path` with each symlink resolved (01M4BR61PPQV7JJE5Y2G9Q90AF). For a
/// path that does not exist yet, it resolves the longest part that
/// exists, and adds the rest.
///
/// ```
/// let dir = tempfile::tempdir().unwrap();
/// let real = dir.path().canonicalize().unwrap();
/// std::os::unix::fs::symlink(&real, real.join("link")).unwrap();
/// assert_eq!(riff::confine::resolve(&real.join("link/new/x")), real.join("new/x"));
/// ```
pub fn resolve(path: &Path) -> PathBuf {
    if let Ok(real) = path.canonicalize() {
        return real;
    }
    match (path.parent(), path.file_name()) {
        (Some(parent), Some(name)) => resolve(parent).join(name),
        _ => path.to_path_buf(),
    }
}

/// The arguments of `riff` that run `program` with `args` in the
/// sandbox of `role`, with the riff server `server`, and the name `name`
/// of the Claude folder and the rules file (default: `RIFF_SESSION`).
///
/// ```
/// use riff::profile::Role;
/// use std::path::Path;
///
/// let args = riff::confine::shim("http://s:1", Role::Lead, Some("lead-o-r"), Path::new("claude"), &[]);
/// assert_eq!(
///     args,
///     ["--server", "http://s:1", "workers", "sandbox", "--role", "lead", "--name", "lead-o-r", "--", "claude"],
/// );
/// ```
pub fn shim(
    server: &str,
    role: Role,
    name: Option<&str>,
    program: &Path,
    args: &[String],
) -> Vec<String> {
    let mut all: Vec<String> = [
        "--server",
        server,
        "workers",
        "sandbox",
        "--role",
        role.name(),
    ]
    .map(String::from)
    .into();
    if let Some(name) = name {
        all.extend(["--name".to_owned(), name.to_owned()]);
    }
    all.push("--".to_owned());
    all.push(program.display().to_string());
    all.extend(args.iter().cloned());
    all
}

/// The profile of `role` for the session of `here`: each path resolved,
/// and checked by [`Profile::of`].
pub fn profile(role: Role, here: &Here) -> Result<Profile> {
    let session = here.session();
    Profile::of(role, &session).with_context(|| format!("riff makes no sandbox for the {role}"))
}

/// The sandbox that riff applied to this process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Applied {
    /// The Landlock ABI of the kernel.
    pub abi: i32,
    /// True when the kernel applied each right that riff asked for.
    pub full: bool,
    /// The number of path rules.
    pub paths: usize,
    /// The number of port rules.
    pub ports: usize,
}

impl fmt::Display for Applied {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        crate::text::sandbox_applied(f, self)
    }
}

/// Restricts this process and each later child to `profile`, with the
/// TCP ports `ports` (01M4BTB72DY0C5Y0EJVR0ZH6FZ). It makes each write
/// path that does not exist yet. A path that riff cannot open gets no
/// rule. It fails when the kernel has no Landlock
/// (01M4BTB74RD24WH59FA65KZ8J0).
pub fn apply(profile: &Profile, ports: &[u16]) -> Result<Applied> {
    let abi = ABI_WANTED;
    let mut ruleset = Ruleset::default()
        .handle_access(AccessFs::from_all(abi))?
        .handle_access(AccessNet::ConnectTcp)?
        .scope(Scope::AbstractUnixSocket | Scope::Signal)?
        .create()?;
    let mut paths = 0;
    let files: Vec<&Path> = profile.write_files().collect();
    for path in profile.write_paths() {
        let _ = match files.contains(&path) {
            true => std::fs::OpenOptions::new()
                .append(true)
                .create(true)
                .open(path)
                .map(drop),
            false => std::fs::create_dir_all(path),
        };
    }
    let grants = profile
        .write_paths()
        .map(|p| (p, AccessFs::from_all(abi)))
        .chain(profile.read_paths().map(|p| (p, AccessFs::from_read(abi))))
        .chain(
            DEVICES
                .iter()
                .map(|d| (Path::new(*d), AccessFs::from_file(abi))),
        );
    for (path, access) in grants {
        let Ok(fd) = PathFd::new(path) else { continue };
        let access = if path.is_dir() {
            access
        } else {
            access & AccessFs::from_file(abi)
        };
        ruleset = ruleset.add_rule(PathBeneath::new(fd, access))?;
        paths += 1;
    }
    for port in ports {
        ruleset = ruleset.add_rule(NetPort::new(*port, AccessNet::ConnectTcp))?;
    }
    let status = ruleset.restrict_self()?;
    let abi = match status.landlock {
        LandlockStatus::Available {
            effective_abi,
            kernel_abi,
        } => kernel_abi.unwrap_or(effective_abi as i32),
        LandlockStatus::NotEnabled | LandlockStatus::NotImplemented => 0,
    };
    if abi == 0 || status.ruleset == RulesetStatus::NotEnforced {
        bail!("{}", crate::text::NO_LANDLOCK);
    }
    no_unix_sockets()?;
    Ok(Applied {
        abi,
        full: status.ruleset == RulesetStatus::FullyEnforced,
        paths,
        ports: ports.len(),
    })
}

/// Sets the core size limit of this process, and so of each later
/// child, to 1 byte, soft and hard (01M4C6HE4D4ADJVBCC6EW8FP0X). A crash
/// in a session then makes no core file and does not start the crash
/// helper of the system (for example apport and its dialog on the
/// desktop of the person). A limit of 0 is not enough: with a pipe in
/// `core_pattern`, the kernel starts the helper for each limit but 1.
pub fn no_core_dumps() -> Result<()> {
    use nix::sys::resource::{Resource, setrlimit};
    setrlimit(Resource::RLIMIT_CORE, 1, 1).context("cannot set the core size limit")
}

/// The git settings of a session, as `GIT_CONFIG_*` variables after the
/// `count` that the environment has (01M4C5RV4J4G0H46GTRVNP5YED). They
/// win over the git settings of the person: a session reads no exclude
/// file in the home, and signs no commit and no tag, because it cannot
/// reach the key agent of the person. A session cannot write the
/// `config` and the `packed-refs` of the clone
/// (01M4CN0W3V733V6R2SG1YYZRCN): so a new branch gets no upstream in
/// the config, and no automatic step packs the refs.
///
/// ```
/// let vars = riff::confine::git_env(Some("1"));
/// assert_eq!(vars[0], ("GIT_CONFIG_COUNT".to_owned(), "7".to_owned()));
/// assert!(vars.contains(&("GIT_CONFIG_KEY_1".to_owned(), "core.excludesFile".to_owned())));
/// assert!(vars.contains(&("GIT_CONFIG_VALUE_1".to_owned(), "/dev/null".to_owned())));
/// assert!(vars.contains(&("GIT_CONFIG_KEY_3".to_owned(), "tag.gpgSign".to_owned())));
/// assert!(vars.contains(&("GIT_CONFIG_KEY_4".to_owned(), "branch.autoSetupMerge".to_owned())));
/// assert_eq!(riff::confine::git_env(Some("x"))[0].1, "6");
/// ```
pub fn git_env(count: Option<&str>) -> Vec<(String, String)> {
    let start: usize = count.and_then(|c| c.parse().ok()).unwrap_or(0);
    let settings = [
        ("core.excludesFile", "/dev/null"),
        ("commit.gpgSign", "false"),
        ("tag.gpgSign", "false"),
        ("branch.autoSetupMerge", "false"),
        ("gc.auto", "0"),
        ("maintenance.auto", "false"),
    ];
    let mut vars = vec![(
        "GIT_CONFIG_COUNT".to_owned(),
        (start + settings.len()).to_string(),
    )];
    for (i, (key, value)) in settings.iter().enumerate() {
        vars.push((format!("GIT_CONFIG_KEY_{}", start + i), (*key).to_owned()));
        vars.push((
            format!("GIT_CONFIG_VALUE_{}", start + i),
            (*value).to_owned(),
        ));
    }
    vars
}

/// A `git` command for riff outside each sandbox: it runs no hook and
/// no fsmonitor of the repository (01M4CN0W644V72YY74WH3KRMMQ), and it
/// goes into no submodule (01M4CXPPVEPFYD86PN9V2Q6EKB). A session can
/// write in a clone, so riff never runs a program that a file of the
/// clone names. A submodule that a session makes in its worktree has a
/// config of its own, and a git in it reads that config.
///
/// ```
/// let git = riff::confine::git();
/// let args: Vec<_> = git.get_args().collect();
/// assert_eq!(args[..4], ["-c", "core.hooksPath=/dev/null", "-c", "core.fsmonitor=false"]);
/// assert!(args.contains(&std::ffi::OsStr::new("diff.ignoreSubmodules=all")));
/// assert!(args.contains(&std::ffi::OsStr::new("submodule.recurse=false")));
/// assert_eq!(git.get_program(), "git");
/// ```
pub fn git() -> std::process::Command {
    let mut git = std::process::Command::new("git");
    for setting in [
        "core.hooksPath=/dev/null",
        "core.fsmonitor=false",
        "diff.ignoreSubmodules=all",
        "submodule.recurse=false",
        "fetch.recurseSubmodules=false",
        "push.recurseSubmodules=no",
        "status.submoduleSummary=false",
    ] {
        git.args(["-c", setting]);
    }
    git
}

/// A [`git`] command for riff in `dir`, with its git dirs from riff
/// and not from the files of a worktree (01M4CW0CEV2ZT3FRET36EB0GMM). A
/// session writes its worktree and `.git/worktrees/NAME`, so it can
/// change the `.git` file and the `commondir` file to name a git dir
/// with a config of its own. A config can name a program (a filter, an
/// ssh command, a credential helper). So:
///
/// - In a worktree `MAIN/.claude/worktrees/NAME`, or below it, riff sets
///   `GIT_DIR=MAIN/.git/worktrees/NAME`, `GIT_COMMON_DIR=MAIN/.git` and
///   `GIT_WORK_TREE` to the worktree. Before, it checks that no part of
///   that path is a symlink, that `MAIN/.git` and
///   `MAIN/.git/worktrees/NAME` are folders, that the clone has no
///   `extensions.worktreeConfig` (a config that a session writes), and
///   that a `commondir` file names `MAIN/.git`: the refs of git read
///   that file also with `GIT_COMMON_DIR`. If a check fails, it gives
///   an error that names the worktree, and runs no git there.
/// - A folder that is a worktree of the agent tool only through a
///   symlink gives an error too.
/// - In each other folder, for example the main clone, riff sets
///   `GIT_COMMON_DIR` to the `.git` folder of `dir` when it has one. So
///   a child git that `git worktree remove` starts in a worktree also
///   reads no `commondir` file.
///
/// ```
/// let dir = tempfile::tempdir().unwrap();
/// let main = dir.path().canonicalize().unwrap();
/// std::fs::create_dir_all(main.join(".git/worktrees/w")).unwrap();
/// std::fs::create_dir_all(main.join(".claude/worktrees/w/src")).unwrap();
/// let git = riff::confine::git_in(&main.join(".claude/worktrees/w/src")).unwrap();
/// let env: std::collections::HashMap<_, _> = git.get_envs().collect();
/// assert_eq!(env[std::ffi::OsStr::new("GIT_DIR")], Some(main.join(".git/worktrees/w").as_os_str()));
/// assert_eq!(env[std::ffi::OsStr::new("GIT_COMMON_DIR")], Some(main.join(".git").as_os_str()));
/// assert_eq!(env[std::ffi::OsStr::new("GIT_WORK_TREE")], Some(main.join(".claude/worktrees/w").as_os_str()));
/// // A worktree that git does not know gives no git.
/// std::fs::create_dir_all(main.join(".claude/worktrees/x")).unwrap();
/// let e = riff::confine::git_in(&main.join(".claude/worktrees/x")).unwrap_err();
/// assert!(format!("{e:#}").contains("no git dir"), "{e:#}");
/// // The main clone keeps its own git dir.
/// let git = riff::confine::git_in(&main).unwrap();
/// let env: std::collections::HashMap<_, _> = git.get_envs().collect();
/// assert_eq!(env[std::ffi::OsStr::new("GIT_COMMON_DIR")], Some(main.join(".git").as_os_str()));
/// assert!(!env.contains_key(std::ffi::OsStr::new("GIT_DIR")));
/// ```
pub fn git_in(dir: &Path) -> Result<std::process::Command> {
    let abs = std::path::absolute(dir).with_context(|| format!("no path {}", dir.display()))?;
    if abs
        .components()
        .any(|c| c == std::path::Component::ParentDir)
    {
        bail!(
            "riff runs no git in {}: the path has a `..` part",
            abs.display()
        );
    }
    let mut git = git();
    git.arg("-C").arg(&abs);
    let lexical = agent_worktree(&abs);
    if lexical.is_none()
        && let Some((main, name)) = abs.canonicalize().ok().as_deref().and_then(agent_worktree)
    {
        bail!(
            "riff runs no git in {}: it is a link to the worktree {}",
            abs.display(),
            main.join(crate::worktrees::AGENT_DIR).join(name).display()
        );
    }
    // A repository in the worktree, for example the clone of a test,
    // holds its own files.
    let agent = lexical.filter(|(main, name)| {
        let tree = main.join(crate::worktrees::AGENT_DIR).join(name);
        !abs.ancestors()
            .take_while(|a| *a != tree)
            .any(|a| a.join(".git").exists())
    });
    let Some((main, name)) = agent else {
        let common = abs.join(".git");
        if common.is_dir() {
            git.env("GIT_COMMON_DIR", common);
        }
        return Ok(git);
    };
    let tree = main.join(crate::worktrees::AGENT_DIR).join(&name);
    let common = main.join(".git");
    let admin = common.join("worktrees").join(&name);
    let refuse =
        |why: &str| anyhow::anyhow!("riff runs no git in the worktree {}: {why}", tree.display());
    for (path, what) in [
        (&tree, "the worktree"),
        (&common, "the git dir of the clone"),
        (&admin, "the git dir of the worktree"),
    ] {
        match path.canonicalize() {
            Ok(real) if real == *path && real.is_dir() => {}
            Ok(_) => return Err(refuse(&format!("{what} is a link or not a folder"))),
            Err(_) => {
                return Err(refuse(&format!(
                    "no git dir: {what} {} is missing",
                    path.display()
                )));
            }
        }
    }
    if !abs.canonicalize().is_ok_and(|real| real.starts_with(&tree)) {
        return Err(refuse(&format!("{} is a link out of it", abs.display())));
    }
    let shared = self::git()
        .args(["config", "--file"])
        .arg(common.join("config"))
        .args(["--type=bool", "--get", "extensions.worktreeConfig"])
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .context("cannot run git")?;
    if String::from_utf8_lossy(&shared.stdout).trim() == "true" {
        return Err(refuse("the clone sets extensions.worktreeConfig"));
    }
    // The refs of git read the `commondir` file also with
    // GIT_COMMON_DIR set.
    let named = std::fs::read_to_string(admin.join("commondir"))
        .ok()
        .map(|text| admin.join(text.trim_end_matches('\n')));
    if named.is_some_and(|named| named.canonicalize().ok() != Some(common.clone())) {
        return Err(refuse("its commondir file names another git dir"));
    }
    git.env("GIT_DIR", admin)
        .env("GIT_COMMON_DIR", common)
        .env("GIT_WORK_TREE", tree);
    Ok(git)
}

/// The name of the Claude folder of the sandbox of [`run_git`]. git
/// does not use it, but each profile has one.
pub const GIT_NAME: &str = "riff-git";

/// A command that runs git in the worktree `tree` of the clone `main`
/// with the sandbox of a worker (01M4D06XN4D19G1B2NBRFH34B3): `riff
/// workers git --worktree TREE --`, and the caller adds the arguments
/// of git. riff uses it for each git step that reads the files of a
/// worktree: a filter or a diff driver that git starts there gets no
/// more rights than the worker had. A push needs the network and the
/// sign-in of the person, so it stays outside with [`git_in_tree`].
pub fn worker_git(main: &Path, tree: &Path) -> Result<std::process::Command> {
    let git = git_in_tree(main, tree)?;
    // A unit test of this crate runs in a test binary, with no riff
    // binary. The tests of the crate run the sandbox.
    if cfg!(test) {
        return Ok(git);
    }
    worker_git_at(&crate::binary::riff()?, main, tree)
}

/// [`worker_git`] with the riff binary `riff`.
///
/// ```
/// let dir = tempfile::tempdir().unwrap();
/// let main = dir.path().canonicalize().unwrap();
/// std::fs::create_dir_all(main.join(".git/worktrees/w")).unwrap();
/// std::fs::create_dir_all(main.join(".claude/worktrees/w")).unwrap();
/// let tree = main.join(".claude/worktrees/w");
/// let riff = std::path::Path::new("/opt/bin/riff");
/// let cmd = riff::confine::worker_git_at(riff, &main, &tree).unwrap();
/// assert_eq!(cmd.get_program(), riff);
/// let args: Vec<_> = cmd.get_args().map(|a| a.to_string_lossy().into_owned()).collect();
/// assert_eq!(args, ["workers", "git", "--worktree", &tree.to_string_lossy(), "--"]);
/// assert!(riff::confine::worker_git_at(riff, &main, dir.path()).is_err());
/// ```
pub fn worker_git_at(riff: &Path, main: &Path, tree: &Path) -> Result<std::process::Command> {
    git_in_tree(main, tree)?;
    let mut cmd = std::process::Command::new(riff);
    cmd.args(["workers", "git", "--worktree"])
        .arg(std::path::absolute(tree)?)
        .arg("--");
    Ok(cmd)
}

/// `riff workers git`: applies the profile of a worker on the worktree
/// `tree` to this process, and then runs git with `args` there, with
/// the git dirs of [`git_in`]. It returns only on an error. With no
/// Landlock, git does not run.
pub fn run_git(server: &str, tree: &Path, args: &[OsString]) -> Result<()> {
    use std::os::unix::process::CommandExt;
    let tree = std::path::absolute(tree)?;
    let Some((main, _)) = agent_worktree(&tree) else {
        bail!(
            "riff runs no git in {}: it is not in {} of a clone",
            tree.display(),
            crate::worktrees::AGENT_DIR
        );
    };
    let mut git = git_in_tree(&main, &tree)?;
    let here = Here::of_dir(&tree, server, Some(GIT_NAME))?;
    // The temp dir of this process can be `/tmp`, which holds the tmux
    // sockets of the person: git gets a temp folder in the riff state.
    let temp = here.state.join(GIT_NAME);
    std::fs::create_dir_all(&temp).with_context(|| format!("cannot make {}", temp.display()))?;
    let here = here.with_temp(&temp);
    let profile = profile(Role::Worker, &here)?;
    no_core_dumps()?;
    apply(&profile, &ports(&profile))?;
    // The git settings of a session: the sandbox cannot read an exclude
    // file in the home or reach the key agent of the person.
    let count = std::env::var("GIT_CONFIG_COUNT").ok();
    git.envs(git_env(count.as_deref())).args(args);
    Err(git.exec()).context("cannot start git")
}

/// [`git_in`] for a worktree `tree` that `git worktree list` of the
/// clone `main` names (01M4CW0CH4F861MWQSQACEX54X). A session writes
/// the `gitdir` file of its worktree, so the list can name any folder,
/// for example one with a `.git` folder of the session. riff runs git
/// only in a worktree that is a folder `NAME` of the worktree folder of
/// `main`, not in a folder below it: that can be a repository of the
/// session.
///
/// ```
/// let dir = tempfile::tempdir().unwrap();
/// let main = dir.path().canonicalize().unwrap();
/// std::fs::create_dir_all(main.join(".git/worktrees/w")).unwrap();
/// std::fs::create_dir_all(main.join(".claude/worktrees/w/sub/.git")).unwrap();
/// assert!(riff::confine::git_in_tree(&main, &main.join(".claude/worktrees/w")).is_ok());
/// let e = riff::confine::git_in_tree(&main, dir.path()).unwrap_err();
/// assert!(format!("{e:#}").contains("not in the worktree folder"), "{e:#}");
/// assert!(riff::confine::git_in_tree(&main, &main.join(".claude/worktrees/w/sub")).is_err());
/// ```
pub fn git_in_tree(main: &Path, tree: &Path) -> Result<std::process::Command> {
    let main = main
        .canonicalize()
        .with_context(|| format!("no clone {}", main.display()))?;
    let abs = std::path::absolute(tree).with_context(|| format!("no path {}", tree.display()))?;
    match agent_worktree(&abs) {
        Some((clone, name))
            if clone == main && abs == main.join(crate::worktrees::AGENT_DIR).join(&name) =>
        {
            git_in(&abs)
        }
        _ => bail!(
            "riff runs no git in {}: it is not in the worktree folder {}",
            abs.display(),
            main.join(crate::worktrees::AGENT_DIR).display()
        ),
    }
}

/// The main clone and the name of the worktree of the agent tool that
/// holds `path`: the last `.claude/worktrees/NAME` of the path. A clone
/// in a worktree, for example a clone of a test, is the repository of
/// its files. A session can make such a clone, so a git step that reads
/// files runs in the sandbox of a worker ([`worker_git`]), and a
/// worktree from `git worktree list` must be in the clone of the list
/// ([`git_in_tree`]).
///
/// ```
/// use std::path::{Path, PathBuf};
/// let of = |p: &str| riff::confine::agent_worktree(Path::new(p));
/// assert_eq!(
///     of("/src/app/.claude/worktrees/w/a/.claude/worktrees/x"),
///     Some((PathBuf::from("/src/app/.claude/worktrees/w/a"), "x".into()))
/// );
/// assert_eq!(of("/src/app/.claude/worktrees/w/src"), Some((PathBuf::from("/src/app"), "w".into())));
/// assert_eq!(of("/src/app/.claude/worktrees"), None);
/// assert_eq!(of("/src/app/.claude/worktrees/.."), None);
/// assert_eq!(of("/src/app"), None);
/// ```
pub fn agent_worktree(path: &Path) -> Option<(PathBuf, std::ffi::OsString)> {
    use std::path::Component;
    let parts: Vec<Component> = path.components().collect();
    parts
        .windows(3)
        .enumerate()
        .rev()
        .find_map(|(i, w)| match w {
            [
                Component::Normal(a),
                Component::Normal(b),
                Component::Normal(name),
            ] if *a == ".claude" && *b == "worktrees" => {
                Some((parts[..i].iter().collect(), (*name).to_owned()))
            }
            _ => None,
        })
}

/// Stops this thread and each later child from making a unix socket
/// with a name: `socket(AF_UNIX, ...)` and `io_uring_setup` fail with
/// `EACCES` (01M4C5AQV8AT8F5WKNF1C9CE5F). A pair of sockets
/// (`socketpair`) still works. Landlock ABI 8 cannot stop a connect to
/// a unix socket with a path, for example the tmux server or the D-Bus
/// of the person, so a seccomp filter does it. `io_uring` can make a
/// socket with no `socket` call, so it goes too.
fn no_unix_sockets() -> Result<()> {
    use nix::libc;
    use seccompiler::{
        BpfProgram, SeccompAction, SeccompCmpArgLen, SeccompCmpOp, SeccompCondition, SeccompFilter,
        SeccompRule, TargetArch,
    };
    let arch: TargetArch = std::env::consts::ARCH
        .try_into()
        .map_err(|e| anyhow::anyhow!("no seccomp for {}: {e:?}", std::env::consts::ARCH))?;
    let unix = SeccompCondition::new(
        0,
        SeccompCmpArgLen::Dword,
        SeccompCmpOp::Eq,
        libc::AF_UNIX as u64,
    )?;
    let rules = std::collections::BTreeMap::from([
        (libc::SYS_socket, vec![SeccompRule::new(vec![unix])?]),
        (libc::SYS_io_uring_setup, vec![]),
    ]);
    let filter = SeccompFilter::new(
        rules,
        SeccompAction::Allow,
        SeccompAction::Errno(libc::EACCES as u32),
        arch,
    )?;
    let program: BpfProgram = filter.try_into()?;
    seccompiler::apply_filter(&program).context("cannot apply the seccomp filter")?;
    Ok(())
}

/// Applies the profile of `role` for this process, with the riff server
/// `server`, and then runs `program` with `args` in the place of this
/// process, with its own Claude folder in [`CLAUDE_CONFIG_DIR`]. It
/// returns only on an error.
pub fn run(
    role: Role,
    server: &str,
    name: Option<&str>,
    program: &Path,
    args: &[OsString],
) -> Result<()> {
    use std::os::unix::process::CommandExt;
    let here = Here::of_dir(&std::env::current_dir()?, server, name)?.with_program(program);
    let profile = profile(role, &here)?;
    let session = here.session();
    // No crash in the session or its test runs starts the crash helper
    // of the system (01M4C6HE4D4ADJVBCC6EW8FP0X).
    no_core_dumps()?;
    // The broker stays outside the sandbox (01M4C5AQGCA3TFZDW23HYKS83S).
    // The session keeps its end of the socket across the exec.
    let broker = crate::broker::start(
        &crate::binary::this_on_disk()?,
        server,
        role,
        &session.worktree,
        &session.clone,
    )?;
    nix::fcntl::fcntl(
        &broker,
        nix::fcntl::FcntlArg::F_SETFD(nix::fcntl::FdFlag::empty()),
    )
    .context("cannot keep the socket of the broker")?;
    let applied = apply(&profile, &ports(&profile))?;
    eprintln!("riff: {applied}");
    // No tmux of the person in a session (01M4C5AQYT6V1MJ37JXJ3PNYQ2).
    let mut cmd = std::process::Command::new(program);
    cmd.args(args)
        .env(CLAUDE_CONFIG_DIR, &session.claude)
        .env(crate::broker::VAR, broker.as_raw_fd().to_string())
        .env_remove("TMUX")
        .env_remove("TMUX_PANE");
    let count = std::env::var("GIT_CONFIG_COUNT").ok();
    cmd.envs(git_env(count.as_deref()));
    // A compile wrapper such as sccache talks to its server outside the
    // sandbox. An empty value also wins over the cargo settings of the
    // person (01M4C5RV4J4G0H46GTRVNP5YED).
    cmd.env("RUSTC_WRAPPER", "")
        .env("CARGO_BUILD_RUSTC_WRAPPER", "");
    // A BASH_ENV that the session cannot read makes each bash print an
    // error (01M4C5RV4J4G0H46GTRVNP5YED).
    if std::env::var_os("BASH_ENV").is_some_and(|f| !profile.reads(&resolve(Path::new(&f)))) {
        cmd.env_remove("BASH_ENV");
    }
    let err = cmd.exec();
    Err(err).with_context(|| format!("cannot start {}", program.display()))
}

/// The text of `riff workers sandbox --show` (01M4BTB7Q1ZT1WD2NMF6BAVWPB):
/// the profile of `role` for this process, and what the kernel applies
/// of it. The apply runs in a thread of its own: Landlock restricts
/// only that thread, so this process stays free.
pub fn show(role: Role, server: &str, name: Option<&str>) -> Result<String> {
    let here = Here::of_dir(&std::env::current_dir()?, server, name)?;
    let profile = profile(role, &here)?;
    let ports = ports(&profile);
    let applied = std::thread::scope(|s| s.spawn(|| apply(&profile, &ports)).join())
        .map_err(|_| anyhow::anyhow!("the test apply of the sandbox stopped"))?;
    Ok(crate::text::sandbox_show(
        &profile,
        &ports,
        applied.as_ref().map_err(|e| format!("{e:#}")),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A clone `main` with the git dir of the worktree `w`, and the
    /// worktree.
    fn clone() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let main = dir.path().canonicalize().unwrap();
        std::fs::create_dir_all(main.join(".git/worktrees/w")).unwrap();
        std::fs::create_dir_all(main.join(".claude/worktrees/w")).unwrap();
        (dir, main)
    }

    fn refused(dir: &Path) -> String {
        format!("{:#}", git_in(dir).unwrap_err())
    }

    /// 01M4CW0CEV2ZT3FRET36EB0GMM: a worktree that is a link, a path
    /// that leaves its worktree, and a link to a worktree give no git.
    #[test]
    fn a_link_or_a_way_out_of_the_worktree_gives_no_git() {
        let (_dir, main) = clone();
        let trees = main.join(crate::worktrees::AGENT_DIR);
        std::os::unix::fs::symlink(&main, trees.join("link")).unwrap();
        std::fs::create_dir_all(main.join(".git/worktrees/link")).unwrap();
        let link = refused(&trees.join("link"));
        assert!(link.contains("is a link"), "{link}");
        let out = refused(&trees.join("w/../../../.git"));
        assert!(out.contains("a `..` part"), "{out}");
        let other = main.join("other");
        std::os::unix::fs::symlink(trees.join("w"), &other).unwrap();
        let to = refused(&other);
        assert!(to.contains("a link to the worktree"), "{to}");
    }

    /// 01M4CW0CEV2ZT3FRET36EB0GMM: with `extensions.worktreeConfig`, git
    /// reads the `config.worktree` that a session writes, so riff runs
    /// no git in a worktree.
    #[test]
    fn a_clone_with_a_config_of_each_worktree_gives_no_git() {
        let (_dir, main) = clone();
        let tree = main.join(crate::worktrees::AGENT_DIR).join("w");
        assert!(git_in(&tree).is_ok());
        std::fs::write(
            main.join(".git/config"),
            "[extensions]\n\tworktreeConfig = true\n",
        )
        .unwrap();
        let why = refused(&tree);
        assert!(why.contains("extensions.worktreeConfig"), "{why}");
    }
}
