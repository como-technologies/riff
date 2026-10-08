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
//! refusal. Today the list has one operation:
//!
//! | Operation | What the broker does |
//! |---|---|
//! | `test-run` | `riff test-run -- PROGRAM ARGS` of the riff outside, in a folder of the worktree of the session |
//!
//! - **One request, one reply socket.** The session sends each request
//!   as one message with four file descriptors: the reply end of a
//!   socket pair of its own, and its stdin, stdout and stderr. So many
//!   processes of the session can ask at the same time.
//! - **The folder.** The folder of a request must be in the root of the
//!   broker: the worktree of the session.
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
use serde::{Deserialize, Serialize};

/// The variable with the file descriptor of the broker in a session.
pub const VAR: &str = "RIFF_BROKER";

/// The operations of the broker (01M4C5AQM63F58YQ9VA391513D).
pub const OPS: [&str; 1] = ["test-run"];

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

/// Why the broker runs nothing for `request` in the root `root`, or
/// `None` (01M4C5AQM63F58YQ9VA391513D).
///
/// ```
/// use riff::broker::{Request, refusal};
///
/// let request = |op: &str, cwd: &str| Request {
///     op: op.into(),
///     args: vec!["cargo".into(), "test".into()],
///     cwd: cwd.into(),
///     env: vec![],
/// };
/// assert_eq!(refusal(&request("test-run", "/w/issue-1"), "/w".as_ref()), None);
/// assert!(refusal(&request("shell", "/w"), "/w".as_ref()).unwrap().contains("no operation shell"));
/// assert!(refusal(&request("test-run", "/etc"), "/w".as_ref()).unwrap().contains("not in"));
/// ```
pub fn refusal(request: &Request, root: &Path) -> Option<String> {
    if !OPS.contains(&request.op.as_str()) {
        return Some(crate::text::broker_no_op(&request.op));
    }
    let cwd = crate::confine::resolve(&request.cwd);
    if !cwd.starts_with(root) {
        return Some(crate::text::broker_not_in(&cwd, root));
    }
    if request.args.is_empty() {
        return Some(crate::text::BROKER_NO_PROGRAM.to_owned());
    }
    None
}

/// Starts the broker outside the sandbox: `riff workers broker --root
/// ROOT`, with the program `riff`. Returns the end of the session, for
/// [`VAR`]. The broker gets the other end as its stdin.
pub fn start(riff: &Path, root: &Path) -> Result<OwnedFd> {
    let (session, broker) = socketpair(
        AddressFamily::Unix,
        SockType::SeqPacket,
        None,
        SockFlag::SOCK_CLOEXEC,
    )
    .context("cannot make the socket pair of the broker")?;
    Command::new(riff)
        .args(["workers", "broker", "--root"])
        .arg(root)
        .env_remove(VAR)
        .stdin(Stdio::from(broker))
        // No pipe of the session stays open in the broker: it lives as
        // long as the last process of the session.
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .context("cannot start the broker")?;
    Ok(session)
}

/// Serves the requests on `socket` for the root `root` until each
/// process of the session closed its end. Each request runs in a thread
/// of its own. `riff` is the riff that runs a test run.
pub fn serve(socket: OwnedFd, root: &Path, riff: &Path) -> Result<()> {
    let root = crate::confine::resolve(root);
    loop {
        let mut buf = vec![0u8; MAX];
        let mut space = nix::cmsg_space!([RawFd; 4]);
        let (len, fds) = {
            let mut iov = [IoSliceMut::new(&mut buf)];
            let msg = recvmsg::<()>(
                socket.as_raw_fd(),
                &mut iov,
                Some(&mut space),
                MsgFlags::MSG_CMSG_CLOEXEC,
            )
            .context("the broker cannot read a request")?;
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
        let fds: Vec<OwnedFd> = fds
            .into_iter()
            .map(|fd| unsafe { OwnedFd::from_raw_fd(fd) })
            .collect();
        if len == 0 && fds.is_empty() {
            return Ok(());
        }
        let (root, riff) = (root.clone(), riff.to_path_buf());
        let request = serde_json::from_slice::<Request>(&buf[..len]);
        std::thread::spawn(move || {
            let mut fds = fds.into_iter();
            let Some(reply) = fds.next() else { return };
            let answer = match request {
                Err(e) => Reply::Refused(format!("a request that riff cannot read: {e}")),
                Ok(request) => answer(&request, &root, &riff, fds.collect()),
            };
            let _ = send(&reply, &answer, &[]);
        });
    }
}

/// Runs `request`, or says why not.
fn answer(request: &Request, root: &Path, riff: &Path, stdio: Vec<OwnedFd>) -> Reply {
    if let Some(why) = refusal(request, root) {
        return Reply::Refused(why);
    }
    let [stdin, stdout, stderr]: [OwnedFd; 3] = match stdio.try_into() {
        Ok(stdio) => stdio,
        Err(_) => return Reply::Refused("a request needs stdin, stdout and stderr".into()),
    };
    let mut cmd = Command::new(riff);
    cmd.arg("test-run")
        .arg("--")
        .args(&request.args)
        .current_dir(&request.cwd)
        .env_remove(VAR)
        .env(
            crate::sandbox::WITHIN_VAR,
            within(root, std::env::var_os("CARGO_TARGET_DIR")),
        )
        .stdin(Stdio::from(stdin))
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr));
    for (name, value) in &request.env {
        if kept_var(name) {
            cmd.env(name, value);
        }
    }
    match cmd.status() {
        Ok(status) => {
            use std::os::unix::process::ExitStatusExt;
            Reply::Code(
                status
                    .code()
                    .unwrap_or_else(|| 128 + status.signal().unwrap_or(0)),
            )
        }
        Err(e) => Reply::Refused(format!("cannot run {}: {e}", riff.display())),
    }
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
    let (mine, theirs) = socketpair(
        AddressFamily::Unix,
        SockType::SeqPacket,
        None,
        SockFlag::SOCK_CLOEXEC,
    )
    .context("cannot make the reply socket")?;
    let body = serde_json::to_vec(request)?;
    let fds = [theirs.as_raw_fd(), 0, 1, 2];
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

/// The broker of this session, from [`VAR`], or `None`.
pub fn here() -> Option<RawFd> {
    std::env::var(VAR).ok()?.parse().ok()
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
        std::thread::spawn(move || serve(broker, &root, &riff));
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

    #[test]
    fn the_broker_keeps_only_the_variables_of_the_tests() {
        let root = tempfile::tempdir().unwrap();
        let seen = root.path().join("seen");
        let riff = root.path().join("riff");
        std::fs::write(
            &riff,
            format!(
                "#!/bin/sh\necho \"[$CARGO_TARGET_DIR] [$MAKEFLAGS] [$LD_PRELOAD] [$RIFF_BROKER] [$RIFF_TEST_RUN_WITHIN] [$CARGO_TERM_COLOR] $*\" > '{}'\n",
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
            ("CARGO_TERM_COLOR".into(), "never".into()),
        ];
        assert_eq!(ask(session.as_raw_fd(), &req).unwrap(), Reply::Code(0));
        // The folders come from the broker (01M4CN0W1F0Y7955C6Q1XB601G):
        // its own target and pool, and its root as the bound of the target.
        let own = |name: &str| std::env::var(name).unwrap_or_default();
        let within = within(
            &crate::confine::resolve(root.path()),
            std::env::var_os("CARGO_TARGET_DIR"),
        );
        assert_eq!(
            std::fs::read_to_string(&seen).unwrap(),
            format!(
                "[{}] [{}] [] [] [{}] [never] test-run -- cargo test\n",
                own("CARGO_TARGET_DIR"),
                own("MAKEFLAGS"),
                within.display()
            )
        );
    }
}
