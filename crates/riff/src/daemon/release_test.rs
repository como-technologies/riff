//! The real-session test on the request of the lead (#1171,
//! 01M5924C8BTT6CW05Q0YZJ1GCY).
//!
//! # Design
//!
//! A container session cannot run `just real-session`: podman does not
//! run inside a container. So the lead asks the daemon of a host with
//! podman: `riff release test vX.Y.Z --host HOST` posts
//! [`Request::ReleaseTest`](super::remote::Request::ReleaseTest) with
//! the commit ([`crate::cycle::test_target`]). The daemon judges the
//! request like each other request of a host: only the person and the
//! lead of the person act on it, never a work session
//! ([`super::remote::judge`]). The daemon is not the author of any
//! item that the test makes.
//!
//! ```mermaid
//! sequenceDiagram
//!     participant L as lead
//!     participant D as riff daemon on HOST
//!     participant G as GitHub
//!     L->>D: release test COMMIT
//!     D->>D: clone origin, detach at COMMIT
//!     D->>G: status riff/real-session pending
//!     D-->>L: started
//!     D->>D: just real-session in the clone
//!     D->>G: status success or failure
//!     L->>G: riff release status
//! ```
//!
//! The daemon replies when the test starts, not when it ends: a test
//! takes longer than a reply waits. It runs one test at a time. The
//! clone is in [`FOLDER`] of its folder; the output of the test is in
//! `FOLDER/COMMIT.log`. After the run, the daemon sets the status
//! `riff/real-session` of the commit: `success` for exit 0, else
//! `failure`. A test that a signal ended also gets `failure`, so no
//! status stays `pending`. [`RUN_VAR`] names another program in place
//! of `just real-session`: a test sets it.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;

use anyhow::{Context, Result, bail};

use super::remote::Served;
use crate::cycle::{TestState, status_args};

/// The folder of the daemon for the clones of the test.
pub const FOLDER: &str = "real-session";

/// The program that the daemon runs in place of `just real-session`.
pub const RUN_VAR: &str = "RIFF_REAL_SESSION_RUN";

/// The commit of the test that runs now.
static RUNNING: Mutex<Option<String>> = Mutex::new(None);

/// True when `text` is a commit ID: 7 to 40 hex digits.
///
/// ```
/// use riff::daemon::release_test::is_commit;
///
/// assert!(is_commit("0123abc"));
/// assert!(!is_commit("main"));
/// assert!(!is_commit("abc"));
/// assert!(!is_commit("--upload-pack=x"));
/// ```
pub fn is_commit(text: &str) -> bool {
    (7..=40).contains(&text.len()) && text.bytes().all(|b| b.is_ascii_hexdigit())
}

fn run(dir: &Path, program: &str, args: &[&str]) -> Result<String> {
    let out = Command::new(program)
        .args(args)
        .current_dir(dir)
        .stdin(Stdio::null())
        .output()
        .with_context(|| format!("cannot run {program}"))?;
    if !out.status.success() {
        bail!(
            "{program} {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
}

fn set_status(clone: &Path, commit: &str, state: TestState, why: &str) -> Result<()> {
    let args = status_args(commit, state, why);
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    run(clone, "gh", &args).map(drop)
}

/// Starts the test on `commit` in the folder `dir` of the daemon: a
/// clone of the `origin` of the main clone of `served`, detached at the
/// commit, and the status `pending`. The run goes on in a thread of its
/// own. It gives the line for the reply.
pub fn start(dir: &Path, served: &Served, commit: &str) -> Result<String> {
    if !is_commit(commit) {
        bail!("\"{commit}\" is not a commit ID");
    }
    {
        let mut running = RUNNING.lock().unwrap();
        if let Some(other) = running.as_ref() {
            bail!("the real-session test runs on {other} already");
        }
        *running = Some(commit.to_owned());
    }
    let started = prepare(dir, served, commit);
    match started {
        Ok(clone) => {
            let owned = commit.to_owned();
            let log = clone.with_extension("log");
            std::thread::spawn(move || {
                finish(&clone, &log, &owned);
                *RUNNING.lock().unwrap() = None;
            });
            Ok(format!(
                "The real-session test started on {commit}. It sets the status {} of the \
                 commit. next: riff release status",
                crate::cycle::REAL_SESSION
            ))
        }
        Err(e) => {
            *RUNNING.lock().unwrap() = None;
            Err(e)
        }
    }
}

fn prepare(dir: &Path, served: &Served, commit: &str) -> Result<PathBuf> {
    let folder = dir.join(FOLDER);
    std::fs::create_dir_all(&folder)?;
    let clone = folder.join(commit);
    if clone.exists() {
        std::fs::remove_dir_all(&clone)?;
    }
    let origin = run(&served.wave.main, "git", &["remote", "get-url", "origin"])?;
    let clone_text = clone.to_string_lossy().into_owned();
    run(
        &folder,
        "git",
        &["clone", "-q", "--no-checkout", &origin, &clone_text],
    )?;
    run(&clone, "git", &["checkout", "-q", "--detach", commit])?;
    let host = &served.wave.host;
    set_status(
        &clone,
        commit,
        TestState::Running,
        &format!("running on {host}"),
    )?;
    Ok(clone)
}

/// Runs the test in `clone`, then sets the final status and removes
/// the clone.
fn finish(clone: &Path, log: &Path, commit: &str) {
    let (state, why) = match run_test(clone, log) {
        Ok(true) => (TestState::Passed, "passed".to_owned()),
        Ok(false) => (
            TestState::Failed,
            "FAIL: see the log of the daemon".to_owned(),
        ),
        Err(e) => (TestState::Failed, format!("FAIL: {e:#}")),
    };
    if let Err(e) = set_status(clone, commit, state, &why) {
        eprintln!("riff daemon: cannot set the status of the real-session test: {e:#}");
    }
    let _ = std::fs::remove_dir_all(clone);
}

fn run_test(clone: &Path, log: &Path) -> Result<bool> {
    let file = std::fs::File::create(log)?;
    let mut command = match std::env::var_os(RUN_VAR) {
        Some(program) => Command::new(program),
        None => {
            let mut command = Command::new("just");
            command.arg("real-session");
            command
        }
    };
    let status = command
        .current_dir(clone)
        .stdin(Stdio::null())
        .stdout(file.try_clone()?)
        .stderr(file)
        .status()
        .context("cannot start the test")?;
    Ok(status.success())
}
