//! The broker: the one door from the sandbox of a session to the world
//! outside it.
//!
//! # Design
//!
//! A session in its sandbox ([`crate::confine`]) cannot make a mount, so
//! it cannot start the bubblewrap of a test run ([`crate::sandbox`]).
//! `riff workers sandbox` starts the broker before it restricts itself:
//! `riff workers broker` stays outside the sandbox. The session gets
//! one end of a socket pair in the variable [`VAR`]
//! (01M4C5AQGCA3TFZDW23HYKS83S).
//!
//! The broker runs only the operations of [`OPS`], never a free
//! command (01M4C5AQM63F58YQ9VA391513D). Each other operation gets a
//! refusal. The list has two operations:
//!
//! | Operation | What the broker does |
//! |---|---|
//! | `test-run` | `riff test-run -- PROGRAM ARGS` of the riff outside, in a folder of the worktree of the session |
//! | `outside` | the one command of a request that an admin approved (`riff outside ask`) |
//!
//! The broker of each role also runs the operations of the own pane
//! ([`crate::door::OWN_OPS`], 01M4DVW24ESG6XCBNMFV7T9Z4E). The broker of
//! a lead (`--role lead`) also runs the operations of the lead in tmux
//! and on the processes of its workers ([`crate::door::LEAD_OPS`],
//! 01M4DDWPC693RNWHY7P7XBZ9TB). The broker of each other role refuses
//! them. The broker waits for each of these operations before it ends,
//! and a hangup does not stop it (01M4DVW2DGX8N9NJZHSAGK5EH4).
//!
//! - **One request, one reply socket.** The session sends each request
//!   as one message with four file descriptors: the reply end of a
//!   socket pair of its own, and its stdin, stdout and stderr. So many
//!   processes of the session can ask at the same time.
//! - **The folder** (01M4DA9PRPZ2JDKC2PK79AT5AA). The folder of a request
//!   must be in the root of the broker: the worktree of the session. The
//!   broker refuses a folder with a `..` part. It runs the operation in
//!   the resolved folder that it checked ([`place`]), not in the folder
//!   of the request, and sets `PWD` to it. So a link that the session
//!   changes after the check does not move the operation.
//! - **The environment.** The broker runs the operation with its own
//!   environment. From the request, it takes only the variables of
//!   [`kept_var`]: the variables of cargo and of the tests. So a request
//!   cannot set `PATH` or `LD_PRELOAD` for a program outside the
//!   sandbox.
//! - **The folders that a test run writes** (01M4CN0W1F0Y7955C6Q1XB601G).
//!   They come from the broker, never from the request: the target is
//!   the `CARGO_TARGET_DIR` of the session (the environment of the
//!   wrapper), else the `target` of the worktree of the folder. The
//!   broker sets [`WITHIN_VAR`](crate::sandbox::WITHIN_VAR), so the
//!   test run refuses a target outside it, also through a symlink. The
//!   pool of build jobs is the `MAKEFLAGS` of the session. So a session
//!   cannot make a test run write in the home of the person.
//! - **The clone and the worktree of a test run**
//!   (01M4D7TB7FZAMASMQG9K7M3Q0D). They come from the broker too:
//!   `riff workers sandbox` gives it the clone and the worktree of the
//!   session at the start, and it sets
//!   [`WORKTREE_VAR`](crate::sandbox::WORKTREE_VAR) and
//!   [`CLONE_VAR`](crate::sandbox::CLONE_VAR). The folder of the request
//!   only picks where the command runs. So no `.git` file or folder that
//!   a session writes makes a test run read the git dir of another
//!   repository ([`crate::sandbox::place`]).
//! - **A command outside the profile** (#614, 01M4DA9PM89KP332T6BR7V0CDT).
//!   `riff outside ask` sends the operation `outside` with the ID of a
//!   request at riff-server. The broker asks riff-server for the
//!   request each [`OUTSIDE_EVERY`], until an admin decides, for at most
//!   [`OUTSIDE_WAIT`]. It runs only the command and the folder that
//!   riff-server gives, one time, with the environment of the broker
//!   and no secret of the session.
//! - **The end.** The broker ends when each process of the session
//!   closed its end of the socket pair.
//!
//! ```mermaid
//! sequenceDiagram
//!     participant T as riff test-run (in the sandbox)
//!     participant B as riff workers broker (outside)
//!     participant R as riff test-run (outside)
//!     T->>B: request, reply socket, stdin, stdout, stderr
//!     B->>B: check the operation, the folder, the variables
//!     B->>R: run, with the stdio of the request
//!     R-->>B: exit code
//!     B-->>T: reply: the exit code, or the refusal
//! ```

use std::ffi::OsString;
use std::io::{IoSlice, IoSliceMut};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};
use nix::sys::socket::{
    AddressFamily, ControlMessage, ControlMessageOwned, MsgFlags, SockFlag, SockType, recvmsg,
    sendmsg, socketpair,
};
use riff_core::wire::{OutsideRequest, OutsideState};
use serde::{Deserialize, Serialize};

/// The variable with the file descriptor of the broker in a session.
pub const VAR: &str = "RIFF_BROKER";

/// The variable with the path of the socket of the broker in a session.
/// The Bash tool of Claude Code gives a command no file descriptor above
/// 2, so a command connects by this path (01M4FCCSXDFSS0EAASN99NT04D).
pub const SOCKET_VAR: &str = "RIFF_BROKER_SOCKET";

/// The operations of the broker (01M4C5AQM63F58YQ9VA391513D).
pub const OPS: [&str; 2] = ["test-run", "outside"];

/// How often the broker asks riff-server for a request of `outside`.
pub const OUTSIDE_EVERY: std::time::Duration = std::time::Duration::from_secs(2);

/// How long the broker waits for an admin to decide a request of
/// `outside`: the life of a request at riff-server.
pub const OUTSIDE_WAIT: std::time::Duration = std::time::Duration::from_secs(60 * 60);

/// The variables of the broker that a command of `outside` does not get:
/// the secrets of the session (01M4CVXJA9WAN5M1RKNGETS8AY).
const SECRET_VARS: [&str; 3] = [
    crate::grant::KEY_VAR,
    crate::grant::GRANT_VAR,
    crate::grant::CLAUDE_TOKEN_VAR,
];

/// The step of the broker that takes a request of `outside` from
/// riff-server, by its ID ([`riff_core::wire::OutsideTake`]).
pub type Take = std::sync::Arc<dyn Fn(&str) -> Result<OutsideRequest> + Send + Sync>;

/// The largest request or reply.
const MAX: usize = 1 << 20;

/// A request to the broker.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Request {
    /// The operation: one of [`OPS`].
    pub op: String,
    /// The arguments of the operation.
    pub args: Vec<OsString>,
    /// The folder of the operation.
    pub cwd: PathBuf,
    /// The variables of the session. The broker keeps only
    /// [`kept_var`].
    pub env: Vec<(OsString, OsString)>,
}

/// The reply of the broker.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Reply {
    /// The operation ran and ended with this exit code.
    Code(i32),
    /// The broker ran nothing, for this reason.
    Refused(String),
}

/// The variables of cargo that a request never sets: each one names a
/// folder that a test run reads or writes, or a program outside
/// (01M4CN0W1F0Y7955C6Q1XB601G). The test run gets the values of the
/// broker.
pub const BROKER_VARS: [&str; 6] = [
    "CARGO_HOME",
    "CARGO_TARGET_DIR",
    "CARGO_BUILD_TARGET_DIR",
    "CARGO_BUILD_BUILD_DIR",
    "CARGO_MAKEFLAGS",
    "CARGO_BUILD_RUSTC_WRAPPER",
];

/// True for a variable of a request that the broker keeps: the
/// variables of cargo and of the Rust tests, and the riff server of the
/// tests. A variable of [`BROKER_VARS`] and `MAKEFLAGS` (the pool of
/// build jobs) come from the broker.
///
/// ```
/// use riff::broker::kept_var;
///
/// assert!(kept_var("CARGO_TERM_COLOR".as_ref()));
/// assert!(kept_var("RUST_TEST_THREADS".as_ref()));
/// assert!(!kept_var("CARGO_TARGET_DIR".as_ref()));
/// assert!(!kept_var("CARGO_BUILD_TARGET_DIR".as_ref()));
/// assert!(!kept_var("MAKEFLAGS".as_ref()));
/// assert!(!kept_var("RIFF_TEST_RUN_WITHIN".as_ref()));
/// assert!(!kept_var("PATH".as_ref()));
/// assert!(!kept_var("LD_PRELOAD".as_ref()));
/// assert!(!kept_var("CARGO_HOME".as_ref()));
/// ```
pub fn kept_var(name: &std::ffi::OsStr) -> bool {
    let Some(name) = name.to_str() else {
        return false;
    };
    let exact = [
        "RIFF_SERVER",
        "DBUS_SESSION_BUS_ADDRESS",
        "TERM",
        "NO_COLOR",
    ];
    exact.contains(&name)
        || (name.starts_with("CARGO_") && !BROKER_VARS.contains(&name))
        || name.starts_with("RUST_TEST_")
        || matches!(name, "RUST_BACKTRACE" | "RUST_LOG")
}

/// The folder that must hold the target of a test run of the broker
/// with the root `root` (01M4CN0W1F0Y7955C6Q1XB601G): the
/// `CARGO_TARGET_DIR` of the session, `own`, else the root.
///
/// ```
/// use riff::broker::within;
/// use std::path::Path;
///
/// assert_eq!(within(Path::new("/w"), None), Path::new("/w"));
/// assert_eq!(within(Path::new("/w"), Some("/t".into())), Path::new("/t"));
/// assert_eq!(within(Path::new("/w"), Some("".into())), Path::new("/w"));
/// ```
pub fn within(root: &Path, own: Option<OsString>) -> PathBuf {
    own.filter(|v| !v.is_empty())
        .map_or_else(|| root.to_path_buf(), PathBuf::from)
}

/// The resolved folder of `cwd`, when it is in the root `root`, else
/// why not (01M4DA9PRPZ2JDKC2PK79AT5AA). A `..` part is refused.
///
/// ```
/// use riff::broker::place;
/// use std::path::Path;
///
/// assert_eq!(place(Path::new("/w/issue-1"), Path::new("/w")), Ok("/w/issue-1".into()));
/// assert!(place(Path::new("/w/../etc"), Path::new("/w")).unwrap_err().contains(".."));
/// assert!(place(Path::new("/w/x/.."), Path::new("/w")).unwrap_err().contains(".."));
/// assert!(place(Path::new("/etc"), Path::new("/w")).unwrap_err().contains("not in"));
/// ```
pub fn place(cwd: &Path, root: &Path) -> Result<PathBuf, String> {
    if cwd
        .components()
        .any(|part| part == std::path::Component::ParentDir)
    {
        return Err(crate::text::broker_parent(cwd));
    }
    let cwd = crate::confine::resolve(cwd);
    if !cwd.starts_with(root) {
        return Err(crate::text::broker_not_in(&cwd, root));
    }
    Ok(cwd)
}

/// The resolved folder where the broker runs `request` in the root
/// `root`, or why the broker runs nothing (01M4C5AQM63F58YQ9VA391513D).
///
/// ```
/// use riff::broker::{Request, check};
///
/// let request = |op: &str, cwd: &str, args: &[&str]| Request {
///     op: op.into(),
///     args: args.iter().map(Into::into).collect(),
///     cwd: cwd.into(),
///     env: vec![],
/// };
/// let test = ["cargo", "test"];
/// assert_eq!(check(&request("test-run", "/w/issue-1", &test), "/w".as_ref()), Ok("/w/issue-1".into()));
/// assert!(check(&request("shell", "/w", &test), "/w".as_ref()).unwrap_err().contains("no operation shell"));
/// assert!(check(&request("test-run", "/etc", &test), "/w".as_ref()).unwrap_err().contains("not in"));
/// assert!(check(&request("test-run", "/w/../etc", &test), "/w".as_ref()).unwrap_err().contains(".."));
/// assert!(check(&request("test-run", "/w", &[]), "/w".as_ref()).is_err());
/// assert!(check(&request("outside", "/w", &["7f3a9c21"]), "/w".as_ref()).is_ok());
/// assert!(check(&request("outside", "/w", &test), "/w".as_ref()).is_err());
/// ```
pub fn check(request: &Request, root: &Path) -> Result<PathBuf, String> {
    if !OPS.contains(&request.op.as_str()) {
        return Err(crate::text::broker_no_op(&request.op));
    }
    let cwd = place(&request.cwd, root)?;
    match request.op.as_str() {
        "outside" if request.args.len() != 1 => Err(crate::text::BROKER_OUTSIDE_ID.to_owned()),
        _ if request.args.is_empty() => Err(crate::text::BROKER_NO_PROGRAM.to_owned()),
        _ => Ok(cwd),
    }
}

/// The path of a new socket of a broker in the own folder `own` of a
/// session: a name that no other process guesses
/// (01M4FCCSXDFSS0EAASN99NT04D). The path is short, so it fits in a
/// `sockaddr_un`.
///
/// ```
/// let own = std::path::Path::new("/run/user/1000/riff/sessions/s1");
/// let path = riff::broker::socket_path(own)?;
/// assert!(path.starts_with(own));
/// assert!(path.to_str().unwrap().len() < 108);
/// assert_ne!(path, riff::broker::socket_path(own)?);
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn socket_path(own: &Path) -> Result<PathBuf> {
    let mut token = [0u8; 8];
    getrandom::fill(&mut token).map_err(|e| anyhow::anyhow!("no random bytes: {e}"))?;
    let name: String = token.iter().map(|b| format!("{b:02x}")).collect();
    Ok(own.join(format!("b-{name}.sock")))
}

/// The longest path that a `sockaddr_un` holds, with its end byte.
const SUN_PATH_MAX: usize = 107;

/// The address of the unix socket at `path`, and the folder that it
/// needs open. A `sockaddr_un` holds at most 107 bytes, and the own
/// folder of a session can be longer than that. Then the address is the
/// name in `/proc/self/fd/N`, where N is the folder of `path` (opened
/// with `O_PATH`): the same socket, with a short path. The folder must
/// stay open as long as the address is in use.
///
/// ```
/// let short = std::path::Path::new("/run/user/1000/riff/sessions/s1/b-0.sock");
/// let (_addr, dir) = riff::broker::address(short)?;
/// assert!(dir.is_none());
///
/// let dir = tempfile::tempdir()?;
/// let long = dir.path().join("x".repeat(120)).join("b-0123456789abcdef.sock");
/// std::fs::create_dir_all(long.parent().unwrap())?;
/// let (addr, held) = riff::broker::address(&long)?;
/// assert!(held.is_some());
/// let path = addr.path().unwrap().to_str().unwrap().to_owned();
/// assert!(path.starts_with("/proc/self/fd/") && path.ends_with("/b-0123456789abcdef.sock"));
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn address(path: &Path) -> Result<(nix::sys::socket::UnixAddr, Option<std::fs::File>)> {
    use nix::sys::socket::UnixAddr;
    use std::os::unix::fs::OpenOptionsExt;
    if path.as_os_str().len() <= SUN_PATH_MAX {
        return Ok((UnixAddr::new(path)?, None));
    }
    let dir = path
        .parent()
        .context("the socket of the broker has no folder")?;
    let name = path
        .file_name()
        .context("the socket of the broker has no name")?;
    let held = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(nix::libc::O_PATH | nix::libc::O_DIRECTORY | nix::libc::O_CLOEXEC)
        .open(dir)
        .with_context(|| format!("cannot open {}", dir.display()))?;
    let short = Path::new("/proc/self/fd")
        .join(held.as_raw_fd().to_string())
        .join(name);
    Ok((UnixAddr::new(&short)?, Some(held)))
}

/// Makes the socket of the broker at `path`, for the user of this
/// process only (mode 0600), and listens on it.
pub fn listen(path: &Path) -> Result<OwnedFd> {
    use nix::sys::socket::{Backlog, bind, listen, socket};
    use std::os::unix::fs::PermissionsExt;
    let fd = socket(
        AddressFamily::Unix,
        SockType::SeqPacket,
        SockFlag::SOCK_CLOEXEC,
        None,
    )
    .context("cannot make the socket of the broker")?;
    let (addr, _dir) = address(path).context("the socket of the broker has a path too long")?;
    bind(fd.as_raw_fd(), &addr).context("cannot bind the socket of the broker")?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
        .context("cannot close the socket of the broker to other users")?;
    listen(&fd, Backlog::new(64)?).context("cannot listen on the socket of the broker")?;
    Ok(fd)
}

/// Starts the broker outside the sandbox: `riff --server SERVER workers
/// broker --root ROOT --clone CLONE --socket SOCKET`, with the program
/// `riff`. Returns the end of the session, for [`VAR`]. The broker gets
/// the other end as its stdin, and listens on `socket` ([`SOCKET_VAR`]).
pub fn start(
    riff: &Path,
    server: &str,
    role: crate::profile::Role,
    root: &Path,
    clone: &Path,
    socket: &Path,
) -> Result<OwnedFd> {
    let (session, broker) = socketpair(
        AddressFamily::Unix,
        SockType::SeqPacket,
        None,
        SockFlag::SOCK_CLOEXEC,
    )
    .context("cannot make the socket pair of the broker")?;
    Command::new(riff)
        .args([
            "--server",
            server,
            "workers",
            "broker",
            "--role",
            role.name(),
        ])
        .arg("--root")
        .arg(root)
        .arg("--clone")
        .arg(clone)
        .arg("--socket")
        .arg(socket)
        .env_remove(VAR)
        .env_remove(SOCKET_VAR)
        .stdin(Stdio::from(broker))
        // No pipe of the session stays open in the broker: it lives as
        // long as the last process of the session.
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .context("cannot start the broker")?;
    Ok(session)
}

/// Serves the requests on `socket` for the root `root` of the clone
/// `clone` until each process of the session closed its end. Each
/// request runs in a thread of its own. `riff` is the riff that runs a
/// test run. `take` takes a request of `outside` from riff-server.
/// `here` has the facts of the operations of [`crate::door`]: only the
/// broker of a lead runs the operations of the lead
/// (01M4DDWPC693RNWHY7P7XBZ9TB). At the end of the session, it waits for
/// each operation of [`crate::door`] that runs
/// (01M4DVW2DGX8N9NJZHSAGK5EH4).
pub fn serve(
    socket: OwnedFd,
    root: &Path,
    clone: &Path,
    riff: &Path,
    take: Take,
    here: crate::door::Here,
) -> Result<()> {
    serve_with(socket, None, root, clone, riff, take, here)
}

/// What a request needs from the broker, for each thread of it.
struct Facts {
    root: PathBuf,
    clone: PathBuf,
    riff: PathBuf,
    take: Take,
    here: crate::door::Here,
    /// The threads of the operations of [`crate::door`]: the end of the
    /// broker waits for them.
    doors: std::sync::Mutex<Vec<std::thread::JoinHandle<()>>>,
}

/// [`serve`], and also the requests of each process that connects to
/// `listener` (01M4FCCSXDFSS0EAASN99NT04D): the sockets of the Bash tool
/// of Claude Code have no file descriptor of the session, so such a
/// command connects by the path of the socket ([`SOCKET_VAR`]). The end
/// stays the end of `socket`.
pub fn serve_with(
    socket: OwnedFd,
    listener: Option<OwnedFd>,
    root: &Path,
    clone: &Path,
    riff: &Path,
    take: Take,
    here: crate::door::Here,
) -> Result<()> {
    let facts = std::sync::Arc::new(Facts {
        root: crate::confine::resolve(root),
        clone: crate::confine::resolve(clone),
        riff: riff.to_path_buf(),
        take,
        here,
        doors: std::sync::Mutex::new(Vec::new()),
    });
    if let Some(listener) = listener {
        let facts = facts.clone();
        std::thread::spawn(move || accept_all(&listener, &facts));
    }
    loop {
        let (body, fds) = receive(&socket).context("the broker cannot read a request")?;
        if body.is_empty() && fds.is_empty() {
            let doors = std::mem::take(&mut *facts.doors.lock().unwrap_or_else(|e| e.into_inner()));
            for door in doors {
                let _ = door.join();
            }
            return Ok(());
        }
        dispatch(&facts, &body, fds);
    }
}

/// Reads one message of `socket`: its body and its file descriptors.
fn receive(socket: &OwnedFd) -> Result<(Vec<u8>, Vec<OwnedFd>)> {
    let mut buf = vec![0u8; MAX];
    let mut space = nix::cmsg_space!([RawFd; 4]);
    let (len, fds) = {
        let mut iov = [IoSliceMut::new(&mut buf)];
        let msg = recvmsg::<()>(
            socket.as_raw_fd(),
            &mut iov,
            Some(&mut space),
            MsgFlags::MSG_CMSG_CLOEXEC,
        )?;
        let mut fds = vec![];
        for cmsg in msg.cmsgs().context("a request with too many files")? {
            if let ControlMessageOwned::ScmRights(raw) = cmsg {
                fds.extend(raw);
            }
        }
        (msg.bytes, fds)
    };
    // SAFETY: SCM_RIGHTS of this recvmsg gave each of these file
    // descriptors to this process, and no other value holds one.
    let fds = fds
        .into_iter()
        .map(|fd| unsafe { OwnedFd::from_raw_fd(fd) })
        .collect();
    buf.truncate(len);
    Ok((buf, fds))
}

/// Runs one request in a thread of its own, and sends the reply to the
/// first file descriptor of the request.
fn dispatch(facts: &std::sync::Arc<Facts>, body: &[u8], fds: Vec<OwnedFd>) {
    let request = serde_json::from_slice::<Request>(body);
    let door = request.as_ref().is_ok_and(|r| crate::door::is_op(&r.op));
    let own = facts.clone();
    let thread = std::thread::spawn(move || {
        let mut fds = fds.into_iter();
        let Some(reply) = fds.next() else { return };
        let answer = match request {
            Err(e) => Reply::Refused(format!("a request that riff cannot read: {e}")),
            Ok(request) if crate::door::is_op(&request.op) => {
                // stdin, stdout, stderr: the door writes its reply
                // to stdout.
                let stdout = fds.nth(1);
                crate::door::answer(&own.here, &request, stdout)
            }
            Ok(request) if request.op == "outside" => outside(
                &request,
                &own.root,
                fds.collect(),
                &own.take,
                OUTSIDE_EVERY,
                OUTSIDE_WAIT,
            ),
            Ok(request) => answer(&request, &own.root, &own.clone, &own.riff, fds.collect()),
        };
        let _ = send(&reply, &answer, &[]);
    });
    if door {
        let mut doors = facts.doors.lock().unwrap_or_else(|e| e.into_inner());
        doors.retain(|d| !d.is_finished());
        doors.push(thread);
    }
}

/// Takes each connection to `listener` for a thread of its own. It
/// serves the requests of a connection until the process closes it. A
/// connection of another user gets nothing.
fn accept_all(listener: &OwnedFd, facts: &std::sync::Arc<Facts>) {
    use nix::sys::socket::{accept4, getsockopt, sockopt::PeerCredentials};
    loop {
        let conn = match accept4(listener.as_raw_fd(), SockFlag::SOCK_CLOEXEC) {
            // SAFETY: accept4 gave this file descriptor to this process.
            Ok(fd) => unsafe { OwnedFd::from_raw_fd(fd) },
            Err(nix::errno::Errno::EINTR | nix::errno::Errno::ECONNABORTED) => continue,
            Err(_) => return,
        };
        let own = getsockopt(&conn, PeerCredentials)
            .is_ok_and(|c| c.uid() == nix::unistd::geteuid().as_raw());
        if !own {
            continue;
        }
        let facts = facts.clone();
        std::thread::spawn(move || {
            while let Ok((body, fds)) = receive(&conn) {
                if body.is_empty() && fds.is_empty() {
                    return;
                }
                dispatch(&facts, &body, fds);
            }
        });
    }
}

/// Makes a hangup do nothing to this process (01M4DVW2DGX8N9NJZHSAGK5EH4):
/// an `end-over-limit` closes the pane of the session, and the broker
/// still ends the session after that. A handler, not `SIG_IGN`, so a
/// program that the broker starts gets the default again at its exec.
pub fn ignore_hangup() -> Result<()> {
    use nix::sys::signal::{SaFlags, SigAction, SigHandler, SigSet, Signal, sigaction};
    extern "C" fn nothing(_: nix::libc::c_int) {}
    let action = SigAction::new(
        SigHandler::Handler(nothing),
        SaFlags::SA_RESTART,
        SigSet::empty(),
    );
    // SAFETY: the handler does nothing, so it is safe in a signal.
    unsafe { sigaction(Signal::SIGHUP, &action) }.context("cannot set the hangup handler")?;
    Ok(())
}

/// Runs `request`, or says why not. The test run gets the worktree and
/// the clone of the session from the broker (01M4D7TB7FZAMASMQG9K7M3Q0D).
fn answer(request: &Request, root: &Path, clone: &Path, riff: &Path, stdio: Vec<OwnedFd>) -> Reply {
    let cwd = match check(request, root) {
        Ok(cwd) => cwd,
        Err(why) => return Reply::Refused(why),
    };
    let [stdin, stdout, stderr]: [OwnedFd; 3] = match stdio.try_into() {
        Ok(stdio) => stdio,
        Err(_) => return Reply::Refused("a request needs stdin, stdout and stderr".into()),
    };
    let mut cmd = Command::new(riff);
    cmd.arg("test-run")
        .arg("--")
        .args(&request.args)
        .current_dir(&cwd)
        .env("PWD", &cwd)
        .env_remove(VAR)
        .env_remove(SOCKET_VAR)
        .env(
            crate::sandbox::WITHIN_VAR,
            within(root, std::env::var_os("CARGO_TARGET_DIR")),
        )
        .env(crate::sandbox::WORKTREE_VAR, root)
        .env(crate::sandbox::CLONE_VAR, clone)
        .stdin(Stdio::from(stdin))
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr));
    for (name, value) in &request.env {
        if kept_var(name) {
            cmd.env(name, value);
        }
    }
    run(&mut cmd, riff)
}

/// Runs `cmd`, and gives its exit code, or 128 and the signal.
fn run(cmd: &mut Command, program: &Path) -> Reply {
    match cmd.status() {
        Ok(status) => {
            use std::os::unix::process::ExitStatusExt;
            Reply::Code(
                status
                    .code()
                    .unwrap_or_else(|| 128 + status.signal().unwrap_or(0)),
            )
        }
        Err(e) => Reply::Refused(format!("cannot run {}: {e}", program.display())),
    }
}

/// The operation `outside` (#614, 01M4DA9PM89KP332T6BR7V0CDT): takes the
/// request of the ID in `request` with `take`, each `every`, until an
/// admin decides, for at most `wait`. Runs the approved command one
/// time, in the folder of riff-server that it checks again.
fn outside(
    request: &Request,
    root: &Path,
    stdio: Vec<OwnedFd>,
    take: &Take,
    every: std::time::Duration,
    wait: std::time::Duration,
) -> Reply {
    if let Err(why) = check(request, root) {
        return Reply::Refused(why);
    }
    let [stdin, stdout, stderr]: [OwnedFd; 3] = match stdio.try_into() {
        Ok(stdio) => stdio,
        Err(_) => return Reply::Refused("a request needs stdin, stdout and stderr".into()),
    };
    let id = request.args[0].to_string_lossy().into_owned();
    let end = std::time::Instant::now() + wait;
    let taken = loop {
        match take(&id) {
            Err(e) => return Reply::Refused(format!("{e:#}")),
            Ok(got) if got.taken => break got,
            Ok(got) if got.state == OutsideState::Denied => {
                return Reply::Refused(crate::text::outside_denied(&got));
            }
            Ok(got) if !got.state.open() => {
                return Reply::Refused(crate::text::outside_closed(&id));
            }
            Ok(_) if std::time::Instant::now() >= end => {
                return Reply::Refused(crate::text::outside_no_decision(&id));
            }
            Ok(_) => std::thread::sleep(every),
        }
    };
    let cwd = match place(Path::new(&taken.cwd), root) {
        Ok(cwd) => cwd,
        Err(why) => return Reply::Refused(why),
    };
    let Some((program, args)) = taken.command.split_first() else {
        return Reply::Refused(crate::text::BROKER_NO_PROGRAM.to_owned());
    };
    let mut cmd = Command::new(program);
    cmd.args(args)
        .current_dir(&cwd)
        .env("PWD", &cwd)
        .env_remove(VAR)
        .env_remove(SOCKET_VAR)
        .stdin(Stdio::from(stdin))
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr));
    for name in SECRET_VARS {
        cmd.env_remove(name);
    }
    run(&mut cmd, Path::new(program))
}

/// Sends `value` as one message on `socket`, with the file descriptors
/// `fds`.
fn send<T: Serialize>(socket: &OwnedFd, value: &T, fds: &[RawFd]) -> Result<()> {
    let body = serde_json::to_vec(value)?;
    let cmsgs = [ControlMessage::ScmRights(fds)];
    let cmsgs: &[ControlMessage] = if fds.is_empty() { &[] } else { &cmsgs };
    sendmsg::<()>(
        socket.as_raw_fd(),
        &[IoSlice::new(&body)],
        cmsgs,
        MsgFlags::empty(),
        None,
    )
    .context("cannot send to the broker")?;
    Ok(())
}

/// Sends `request` to the broker on the file descriptor `broker`, with
/// the stdio of this process, and waits for the reply.
pub fn ask(broker: RawFd, request: &Request) -> Result<Reply> {
    ask_with(broker, request, [0, 1, 2])
}

/// [`ask`] with `stdio` as the stdin, the stdout and the stderr of the
/// request.
pub fn ask_with(broker: RawFd, request: &Request, stdio: [RawFd; 3]) -> Result<Reply> {
    let (mine, theirs) = socketpair(
        AddressFamily::Unix,
        SockType::SeqPacket,
        None,
        SockFlag::SOCK_CLOEXEC,
    )
    .context("cannot make the reply socket")?;
    let body = serde_json::to_vec(request)?;
    let fds = [theirs.as_raw_fd(), stdio[0], stdio[1], stdio[2]];
    sendmsg::<()>(
        broker,
        &[IoSlice::new(&body)],
        &[ControlMessage::ScmRights(&fds)],
        MsgFlags::empty(),
        None,
    )
    .context("cannot reach the broker")?;
    drop(theirs);
    let mut buf = vec![0u8; MAX];
    let len = {
        let mut iov = [IoSliceMut::new(&mut buf)];
        recvmsg::<()>(mine.as_raw_fd(), &mut iov, None, MsgFlags::empty())
            .context("no reply of the broker")?
            .bytes
    };
    if len == 0 {
        bail!("the broker ended with no reply");
    }
    Ok(serde_json::from_slice(&buf[..len])?)
}

/// The broker of this session, or `None`: a connection to the socket of
/// [`SOCKET_VAR`] (01M4FCCSXDFSS0EAASN99NT04D), else the file descriptor
/// of [`VAR`]. The connection stays open as long as this process.
pub fn here() -> Option<RawFd> {
    static HERE: std::sync::OnceLock<Option<RawFd>> = std::sync::OnceLock::new();
    *HERE.get_or_init(|| {
        let path = std::env::var_os(SOCKET_VAR).filter(|p| !p.is_empty());
        path.and_then(|p| connect(Path::new(&p)).ok())
            .or_else(|| std::env::var(VAR).ok()?.parse().ok())
    })
}

/// Connects to the socket of a broker at `path`. The broker makes the
/// socket at its start, so a connect that finds no socket yet tries
/// again for a short time.
pub fn connect(path: &Path) -> Result<RawFd> {
    use nix::sys::socket::{connect, socket};
    use std::os::fd::IntoRawFd;
    let (addr, _dir) = address(path)?;
    let mut tries = 0;
    loop {
        let fd = socket(
            AddressFamily::Unix,
            SockType::SeqPacket,
            SockFlag::SOCK_CLOEXEC,
            None,
        )
        .context("cannot make the socket to the broker")?;
        match connect(fd.as_raw_fd(), &addr) {
            Ok(()) => return Ok(fd.into_raw_fd()),
            Err(nix::errno::Errno::ENOENT | nix::errno::Errno::ECONNREFUSED) if tries < 10 => {
                tries += 1;
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            Err(e) => return Err(e).context("cannot connect to the broker"),
        }
    }
}

/// `riff test-run PROGRAM ARGS` in a session with a broker: asks the
/// broker to run it outside the sandbox. Returns the exit code.
pub fn test_run(broker: RawFd, program: &std::ffi::OsStr, args: &[OsString]) -> Result<i32> {
    let request = Request {
        op: "test-run".into(),
        args: std::iter::once(program.to_owned())
            .chain(args.iter().cloned())
            .collect(),
        cwd: std::env::current_dir()?,
        env: std::env::vars_os().collect(),
    };
    match ask(broker, &request)? {
        Reply::Code(code) => Ok(code),
        Reply::Refused(why) => bail!("{}", crate::text::broker_refused(&why)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A broker in a thread of this test, for the root `root`, with the
    /// program `riff`.
    fn broker(root: &Path, riff: &Path) -> OwnedFd {
        let (session, broker) = socketpair(
            AddressFamily::Unix,
            SockType::SeqPacket,
            None,
            SockFlag::SOCK_CLOEXEC,
        )
        .unwrap();
        let (root, riff) = (root.to_path_buf(), riff.to_path_buf());
        let take: Take = std::sync::Arc::new(|_| anyhow::bail!("no riff-server in this test"));
        let here = crate::door::Here {
            server: "http://127.0.0.1:9".into(),
            tmux: None,
            lead: None,
        };
        std::thread::spawn(move || serve(broker, &root, &root, &riff, take, here));
        session
    }

    fn request(op: &str, cwd: &Path) -> Request {
        Request {
            op: op.into(),
            args: vec!["true".into()],
            cwd: cwd.into(),
            env: vec![],
        }
    }

    #[test]
    fn an_unknown_operation_is_refused() {
        let root = tempfile::tempdir().unwrap();
        let ran = root.path().join("ran");
        let riff = root.path().join("riff");
        std::fs::write(&riff, format!("#!/bin/sh\ntouch '{}'\n", ran.display())).unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&riff, std::fs::Permissions::from_mode(0o755)).unwrap();
        let session = broker(root.path(), &riff);
        for op in ["shell", "tmux", "", "test-run "] {
            let reply = ask(session.as_raw_fd(), &request(op, root.path())).unwrap();
            assert!(
                matches!(&reply, Reply::Refused(why) if why.contains("no operation")),
                "{op}: {reply:?}"
            );
        }
        assert!(!ran.exists(), "the broker ran an unknown operation");

        let reply = ask(session.as_raw_fd(), &request("test-run", Path::new("/"))).unwrap();
        assert!(
            matches!(&reply, Reply::Refused(why) if why.contains("not in")),
            "{reply:?}"
        );
        assert!(!ran.exists(), "the broker ran outside its root");

        let reply = ask(session.as_raw_fd(), &request("test-run", root.path())).unwrap();
        assert_eq!(reply, Reply::Code(0));
        assert!(ran.exists(), "the broker did not run test-run");
    }

    /// 01M4FCCSXDFSS0EAASN99NT04D: a process that connects to the path
    /// socket of the broker asks it as it does with the file descriptor.
    /// The socket is closed to other users, and a connection with no
    /// request ends with no effect on the broker.
    #[test]
    fn a_process_asks_the_broker_by_the_path_of_its_socket() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().unwrap();
        let root = crate::confine::resolve(tmp.path());
        let ran = root.join("ran");
        let riff = root.join("riff");
        std::fs::write(&riff, format!("#!/bin/sh\ntouch '{}'\n", ran.display())).unwrap();
        std::fs::set_permissions(&riff, std::fs::Permissions::from_mode(0o755)).unwrap();
        let path = socket_path(&root).unwrap();
        let listener = match listen(&path) {
            Ok(listener) => listener,
            // A test inside a session of an older riff: its seccomp
            // filter stops each unix socket with a name. The Gate runs
            // this test outside a sandbox.
            Err(e) if format!("{e:#}").contains("EACCES") => {
                println!("skip: this process can make no unix socket: {e:#}");
                return;
            }
            Err(e) => panic!("{e:#}"),
        };
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        let (session, pair) = socketpair(
            AddressFamily::Unix,
            SockType::SeqPacket,
            None,
            SockFlag::SOCK_CLOEXEC,
        )
        .unwrap();
        let take: Take = std::sync::Arc::new(|_| anyhow::bail!("no riff-server in this test"));
        let here = crate::door::Here {
            server: "http://127.0.0.1:9".into(),
            tmux: None,
            lead: None,
        };
        let (r, c) = (root.clone(), riff.clone());
        std::thread::spawn(move || serve_with(pair, Some(listener), &r, &r, &c, take, here));

        // A connection that sends nothing, and then closes.
        drop(unsafe_free(connect(&path).unwrap()));
        let fd = connect(&path).unwrap();
        let reply = ask(fd, &request("test-run", &root)).unwrap();
        assert_eq!(reply, Reply::Code(0));
        assert!(ran.exists(), "the broker did not run test-run");
        // The same connection asks again; a refusal comes back too.
        let reply = ask(fd, &request("shell", &root)).unwrap();
        assert!(matches!(&reply, Reply::Refused(why) if why.contains("no operation")));
        drop(session);
    }

    /// The own folder of a session can be longer than a `sockaddr_un`
    /// holds: the socket still works, and a failed listen does not end
    /// the broker (the tests of the Gate had such a folder).
    #[test]
    fn a_socket_in_a_long_folder_still_works() {
        let tmp = tempfile::tempdir().unwrap();
        let root = crate::confine::resolve(tmp.path());
        let own = root.join("x".repeat(60)).join("y".repeat(60));
        std::fs::create_dir_all(&own).unwrap();
        let path = socket_path(&own).unwrap();
        assert!(path.as_os_str().len() > 108, "{}", path.display());
        let listener = match listen(&path) {
            Ok(listener) => listener,
            Err(e) if format!("{e:#}").contains("EACCES") => {
                println!("skip: this process can make no unix socket: {e:#}");
                return;
            }
            Err(e) => panic!("{e:#}"),
        };
        let (_session, pair) = socketpair(
            AddressFamily::Unix,
            SockType::SeqPacket,
            None,
            SockFlag::SOCK_CLOEXEC,
        )
        .unwrap();
        let riff = root.join("riff");
        std::fs::write(&riff, "#!/bin/sh\n").unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&riff, std::fs::Permissions::from_mode(0o755)).unwrap();
        let take: Take = std::sync::Arc::new(|_| anyhow::bail!("no riff-server in this test"));
        let here = crate::door::Here {
            server: "http://127.0.0.1:9".into(),
            tmux: None,
            lead: None,
        };
        let (r, c) = (root.clone(), riff.clone());
        std::thread::spawn(move || serve_with(pair, Some(listener), &r, &r, &c, take, here));
        let fd = connect(&path).unwrap();
        let reply = ask(fd, &request("test-run", &root)).unwrap();
        assert_eq!(reply, Reply::Code(0));
    }

    /// Takes the ownership of a connected socket, for a drop.
    fn unsafe_free(fd: RawFd) -> OwnedFd {
        // SAFETY: `connect` gave this file descriptor to the caller, and
        // no other value holds it.
        unsafe { OwnedFd::from_raw_fd(fd) }
    }

    /// A fake `riff` in `dir` that writes its `PWD` to the file `seen`.
    fn pwd_riff(dir: &Path, seen: &Path) -> PathBuf {
        let riff = dir.join("riff");
        std::fs::write(
            &riff,
            format!("#!/bin/sh\necho \"$PWD\" > '{}'\n", seen.display()),
        )
        .unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&riff, std::fs::Permissions::from_mode(0o755)).unwrap();
        riff
    }

    #[test]
    fn the_broker_refuses_a_parent_part_and_runs_in_the_folder_that_it_checked() {
        let tmp = tempfile::tempdir().unwrap();
        let root = crate::confine::resolve(tmp.path());
        let (sub, link) = (root.join("sub"), root.join("link"));
        std::fs::create_dir(&sub).unwrap();
        std::os::unix::fs::symlink(&sub, &link).unwrap();
        let seen = root.join("seen");
        let session = broker(&root, &pwd_riff(&root, &seen));

        // A `..` part is refused, also one that stays in the root.
        for cwd in [root.join("sub/.."), root.join("../x"), link.join("..")] {
            let reply = ask(session.as_raw_fd(), &request("test-run", &cwd)).unwrap();
            assert!(
                matches!(&reply, Reply::Refused(why) if why.contains("..")),
                "{}: {reply:?}",
                cwd.display()
            );
        }
        assert!(!seen.exists(), "the broker ran a request with a .. part");

        // The run is in the resolved folder, not in the link of the
        // request: a change of the link after the check moves nothing.
        let reply = ask(session.as_raw_fd(), &request("test-run", &link)).unwrap();
        assert_eq!(reply, Reply::Code(0));
        assert_eq!(
            std::fs::read_to_string(&seen).unwrap().trim(),
            sub.to_str().unwrap()
        );
    }

    /// The stdio of a request of a test: no stdin, and stdout and stderr
    /// to `out`.
    fn stdio(out: &Path) -> Vec<OwnedFd> {
        let file = std::fs::File::create(out).unwrap();
        vec![
            std::fs::File::open("/dev/null").unwrap().into(),
            file.try_clone().unwrap().into(),
            file.into(),
        ]
    }

    /// A request of the server with the state `state`, the folder `cwd`
    /// and the command `command`.
    fn outside_request(state: OutsideState, cwd: &Path, command: &[&str]) -> OutsideRequest {
        OutsideRequest {
            id: "7f3a9c21".into(),
            by: "riff://mike@pangolin/acme/app?session=a6cf"
                .parse()
                .unwrap(),
            command: command.iter().map(|c| c.to_string()).collect(),
            cwd: cwd.to_string_lossy().into_owned(),
            reason: "a test".into(),
            state,
            decided_by: Some("dan".into()),
            taken: state == OutsideState::Ran,
        }
    }

    /// A take that gives `replies` in turn, then the last one again.
    fn takes(replies: Vec<OutsideRequest>) -> Take {
        let replies = std::sync::Mutex::new(replies);
        std::sync::Arc::new(move |id: &str| {
            assert_eq!(id, "7f3a9c21");
            let mut replies = replies.lock().unwrap();
            Ok(if replies.len() > 1 {
                replies.remove(0)
            } else {
                replies[0].clone()
            })
        })
    }

    fn outside_of(root: &Path, take: &Take, out: &Path) -> Reply {
        let mut req = request("outside", root);
        req.args = vec!["7f3a9c21".into()];
        let short = std::time::Duration::from_millis(10);
        outside(&req, root, stdio(out), take, short, short * 20)
    }

    #[test]
    fn the_broker_runs_an_approved_command_one_time_in_the_folder_of_the_server() {
        let tmp = tempfile::tempdir().unwrap();
        let root = crate::confine::resolve(tmp.path());
        let (sub, link) = (root.join("sub"), root.join("link"));
        std::fs::create_dir(&sub).unwrap();
        std::os::unix::fs::symlink(&sub, &link).unwrap();
        let out = root.join("out");
        let command = ["sh", "-c", "echo \"$PWD [$RIFF_BROKER]\"; exit 3"];
        let take = takes(vec![
            outside_request(OutsideState::Asked, &link, &command),
            outside_request(OutsideState::Ran, &link, &command),
        ]);
        assert_eq!(outside_of(&root, &take, &out), Reply::Code(3));
        assert_eq!(
            std::fs::read_to_string(&out).unwrap(),
            format!("{} []\n", sub.display()),
            "the command runs in the resolved folder, with no broker"
        );
    }

    #[test]
    fn the_broker_runs_nothing_that_is_denied_old_or_outside_its_root() {
        let tmp = tempfile::tempdir().unwrap();
        let root = crate::confine::resolve(tmp.path());
        let out = root.join("out");
        let ran = root.join("ran");
        let command = ["touch", ran.to_str().unwrap()];
        let cases = [
            (OutsideState::Denied, root.clone(), "dan denied"),
            (OutsideState::Asked, root.clone(), "no admin decided"),
            (OutsideState::Ran, PathBuf::from("/"), "not in"),
            (OutsideState::Ran, root.join("x/.."), ".."),
        ];
        for (state, cwd, why) in cases {
            let take = takes(vec![outside_request(state, &cwd, &command)]);
            let reply = outside_of(&root, &take, &out);
            assert!(
                matches!(&reply, Reply::Refused(text) if text.contains(why)),
                "{state:?} in {}: {reply:?}",
                cwd.display()
            );
        }
        // A request that ran before, with no `taken`, does not run again.
        let mut old = outside_request(OutsideState::Ran, &root, &command);
        old.taken = false;
        let reply = outside_of(&root, &takes(vec![old]), &out);
        assert!(
            matches!(&reply, Reply::Refused(text) if text.contains("ran before")),
            "{reply:?}"
        );
        assert!(
            !ran.exists(),
            "the broker ran a command that it must refuse"
        );
    }

    #[test]
    fn the_broker_keeps_only_the_variables_of_the_tests() {
        let root = tempfile::tempdir().unwrap();
        let seen = root.path().join("seen");
        let riff = root.path().join("riff");
        std::fs::write(
            &riff,
            format!(
                "#!/bin/sh\necho \"[$CARGO_TARGET_DIR] [$MAKEFLAGS] [$LD_PRELOAD] [$RIFF_BROKER] [$RIFF_TEST_RUN_WITHIN] [$RIFF_TEST_RUN_WORKTREE] [$RIFF_TEST_RUN_CLONE] [$CARGO_TERM_COLOR] $*\" > '{}'\n",
                seen.display()
            ),
        )
        .unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&riff, std::fs::Permissions::from_mode(0o755)).unwrap();
        let session = broker(root.path(), &riff);
        let mut req = request("test-run", root.path());
        req.args = vec!["cargo".into(), "test".into()];
        req.env = vec![
            ("CARGO_TARGET_DIR".into(), "/t".into()),
            (
                "MAKEFLAGS".into(),
                "--jobserver-auth=fifo:/h/.local/bin/f".into(),
            ),
            ("LD_PRELOAD".into(), "/evil.so".into()),
            ("RIFF_BROKER".into(), "9".into()),
            ("RIFF_TEST_RUN_WITHIN".into(), "/".into()),
            ("RIFF_TEST_RUN_WORKTREE".into(), "/".into()),
            ("RIFF_TEST_RUN_CLONE".into(), "/home".into()),
            ("CARGO_TERM_COLOR".into(), "never".into()),
        ];
        assert_eq!(ask(session.as_raw_fd(), &req).unwrap(), Reply::Code(0));
        // The folders come from the broker (01M4CN0W1F0Y7955C6Q1XB601G,
        // 01M4D7TB7FZAMASMQG9K7M3Q0D): its own target and pool, its root
        // as the bound of the target, and its root and clone.
        let own = |name: &str| std::env::var(name).unwrap_or_default();
        let within = within(
            &crate::confine::resolve(root.path()),
            std::env::var_os("CARGO_TARGET_DIR"),
        );
        assert_eq!(
            std::fs::read_to_string(&seen).unwrap(),
            format!(
                "[{}] [{}] [] [] [{}] [{3}] [{3}] [never] test-run -- cargo test\n",
                own("CARGO_TARGET_DIR"),
                own("MAKEFLAGS"),
                within.display(),
                crate::confine::resolve(root.path()).display()
            )
        );
    }
}
