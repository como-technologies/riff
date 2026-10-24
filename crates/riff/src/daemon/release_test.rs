//! The real-session test on the request of the lead (#1171,
//! 01M5924C8BTT6CW05Q0YZJ1GCY, 01M5MR8CM887EFGMKZM5BQA9JJ).
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
//!     D->>D: just real-session in the clone, RIFF_SERVER of the repository
//!     D->>G: status success or failure
//!     L->>G: riff release status
//! ```
//!
//! The daemon replies when the test starts, not when it ends: a test
//! takes longer than a reply waits. It runs one test at a time, and
//! [`running`] gives it: its tag, its commit and its seconds
//! (01M5QVBTXE8YRY44A1C1VP71QS). [`Status`](super::Status) holds it, so
//! `riff daemon status`, `riff view`, `riff top` and `riff who` show it,
//! and [`Status::busy`](super::Status::busy) counts it as one. The
//! clone is in [`FOLDER`] of its folder; the output of the test is in
//! `FOLDER/COMMIT.log`. After the run, the daemon sets the status
//! `riff/real-session` of the commit: `success` for exit 0, else
//! `failure`. A test that a signal ended also gets `failure`, so no
//! status stays `pending`. [`RUN_VAR`] names another program in place
//! of `just real-session`: a test sets it.
//!
//! A test that fails also writes its reason (#1255): the daemon reads
//! the failed step in the log (`FAIL at ...`), comments it on the
//! release issue of the tag with the last 30 lines of the
//! log, and sets the failed step as the description of the status. The
//! lead in a container reads it with no access to the journal of the
//! host. [`redact`] cuts the secrets out of the text first.
//!
//! The daemon runs with its own environment, and a systemd unit sets no
//! `RIFF_SERVER`. So the test gets `RIFF_SERVER` from the repository of
//! the request: the `server` of the repository in `served.json`
//! ([`super::remote::SERVED_FILE`]). `riff run` in the test then reaches
//! the riff-server of the repository, also when it is not on the host
//! (01M5MR8CM887EFGMKZM5BQA9JJ). See [`command`].

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::Instant;

use anyhow::{Context, Result, bail};
use riff_client::daemons::Test;

use super::remote::Served;
use crate::cycle::{TestState, status_args};

/// The folder of the daemon for the clones of the test.
pub const FOLDER: &str = "real-session";

/// The program that the daemon runs in place of `just real-session`.
pub const RUN_VAR: &str = "RIFF_REAL_SESSION_RUN";

/// The test that runs now.
struct Started {
    tag: String,
    commit: String,
    at: Instant,
}

impl Started {
    /// The test as the status of the daemon shows it.
    fn shown(&self) -> Test {
        Test {
            tag: self.tag.clone(),
            commit: self.commit.clone(),
            secs: self.at.elapsed().as_secs(),
        }
    }
}

/// The test that runs now: the daemon runs one at a time.
static RUNNING: Mutex<Option<Started>> = Mutex::new(None);

fn running_lock() -> MutexGuard<'static, Option<Started>> {
    RUNNING.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The test that runs now, or `None`. [`Status`](super::Status) shows
/// it, and [`Status::busy`](super::Status::busy) counts it as one
/// (01M5QVBTXE8YRY44A1C1VP71QS).
///
/// ```
/// assert_eq!(riff::daemon::release_test::running(), None);
/// ```
pub fn running() -> Option<Test> {
    running_lock().as_ref().map(Started::shown)
}

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
pub fn start(dir: &Path, served: &Served, commit: &str, tag: &str) -> Result<String> {
    if !is_commit(commit) {
        bail!("\"{commit}\" is not a commit ID");
    }
    let tag = crate::cycle::tag(crate::cycle::tag_version(tag)?);
    {
        let mut running = running_lock();
        if let Some(other) = running.as_ref() {
            bail!("the real-session test runs on {} already", other.commit);
        }
        *running = Some(Started {
            tag: tag.clone(),
            commit: commit.to_owned(),
            at: Instant::now(),
        });
    }
    let started = prepare(dir, served, commit);
    match started {
        Ok(clone) => {
            let owned = commit.to_owned();
            let log = clone.with_extension("log");
            let server = served.wave.server.clone();
            std::thread::spawn(move || {
                finish(&clone, &log, (&owned, &tag), &server);
                *running_lock() = None;
            });
            Ok(format!(
                "The real-session test started on {commit}. It sets the status {} of the \
                 commit. next: riff release status",
                crate::cycle::REAL_SESSION
            ))
        }
        Err(e) => {
            *running_lock() = None;
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

/// Runs the test in `clone` on the riff-server `server`, then sets the
/// final status and removes the clone. A fail goes on the release issue
/// of the tag with the failed step and the end of the log, and its step
/// is the description of the status (#1255).
fn finish(clone: &Path, log: &Path, (commit, tag): (&str, &str), server: &str) {
    let (state, why) = match run_test(clone, log, server) {
        Ok(true) => (TestState::Passed, "passed".to_owned()),
        Ok(false) => {
            let text = std::fs::read_to_string(log).unwrap_or_default();
            let step = failed_step(&text)
                .unwrap_or_else(|| "the test ended with a failure, see the comment".to_owned());
            let comment = fail_comment(tag, commit, &step, &text, TAIL_LINES);
            if let Err(e) = comment_failure(clone, tag, &comment) {
                eprintln!("riff daemon: cannot write the fail of the test on the issue: {e:#}");
            }
            (TestState::Failed, format!("FAIL at {}", redact(&step)))
        }
        Err(e) => {
            let step = format!("{e:#}");
            let comment = fail_comment(tag, commit, &step, "", TAIL_LINES);
            if let Err(e) = comment_failure(clone, tag, &comment) {
                eprintln!("riff daemon: cannot write the fail of the test on the issue: {e:#}");
            }
            (TestState::Failed, format!("FAIL: {}", redact(&step)))
        }
    };
    if let Err(e) = set_status(clone, commit, state, &why) {
        eprintln!("riff daemon: cannot set the status of the real-session test: {e:#}");
    }
    let _ = std::fs::remove_dir_all(clone);
}

/// How many last lines of the log the comment of a fail holds.
const TAIL_LINES: usize = 30;

/// `text` with each secret cut out: a word that starts like a token of
/// GitHub or Anthropic, `Bearer X`, and the value of `NAME=value` when
/// NAME names a token, a key, a secret or a password.
///
/// ```
/// use riff::daemon::release_test::redact;
///
/// assert_eq!(redact("push ghp_abc123 now"), "push [secret] now");
/// assert_eq!(redact("GH_TOKEN=abc ok"), "GH_TOKEN=[secret] ok");
/// assert_eq!(redact("Authorization: Bearer xyz"), "Authorization: Bearer [secret]");
/// assert_eq!(redact("step 6 of 10"), "step 6 of 10");
/// ```
pub fn redact(text: &str) -> String {
    const PREFIXES: [&str; 8] = [
        "ghp_",
        "gho_",
        "ghs_",
        "ghu_",
        "ghr_",
        "github_pat_",
        "sk-ant-",
        "sk-",
    ];
    const NAMES: [&str; 4] = ["TOKEN", "KEY", "SECRET", "PASSWORD"];
    let mut out = Vec::new();
    let mut after_bearer = false;
    for line in text.lines() {
        let mut words = Vec::new();
        for word in line.split(' ') {
            let upper = word.to_ascii_uppercase();
            let word = if after_bearer && !word.is_empty() {
                "[secret]".to_owned()
            } else if let Some((name, value)) = word.split_once('=')
                && !value.is_empty()
                && NAMES.iter().any(|n| name.to_ascii_uppercase().contains(n))
            {
                format!("{name}=[secret]")
            } else if PREFIXES.iter().any(|p| word.starts_with(p)) {
                "[secret]".to_owned()
            } else {
                word.to_owned()
            };
            after_bearer = upper == "BEARER";
            words.push(word);
        }
        after_bearer = false;
        out.push(words.join(" "));
    }
    out.join("\n")
}

/// The failed step in the log of the test: the text after the last
/// `FAIL at`, that `just real-session` writes.
///
/// ```
/// use riff::daemon::release_test::failed_step;
///
/// let log = "a\nreal-session: FAIL at step 6 of 10, run the item: exit 1\nb\n";
/// assert_eq!(failed_step(log), Some("step 6 of 10, run the item: exit 1".to_owned()));
/// assert_eq!(failed_step("nothing"), None);
/// ```
pub fn failed_step(log: &str) -> Option<String> {
    log.lines().rev().find_map(|l| {
        l.split_once("FAIL at ")
            .map(|(_, step)| step.trim().to_owned())
    })
}

/// The comment on the release issue for a test of `tag` on `commit`
/// that failed at `step`, with the last lines of its `log` and no secret.
///
/// ```
/// use riff::daemon::release_test::fail_comment;
///
/// let text = fail_comment("v1.2.3", "abc1234", "step 2 of 10", "a\nb\nc", 2);
/// assert!(text.starts_with("The real-session test of v1.2.3 failed on abc1234.\n"));
/// assert!(text.contains("Failed step: step 2 of 10"));
/// assert!(text.contains("```\nb\nc\n```"));
/// assert!(!text.contains("\na\n"));
/// ```
pub fn fail_comment(tag: &str, commit: &str, step: &str, log: &str, lines: usize) -> String {
    let all: Vec<&str> = log.lines().collect();
    let tail = redact(&all[all.len().saturating_sub(lines)..].join("\n"));
    format!(
        "The real-session test of {tag} failed on {commit}.\n\nFailed step: {}\n\n\
         The last lines of its output:\n\n```\n{tail}\n```\n",
        redact(step)
    )
}

/// Writes the reason of a fail on the release issue of `tag`. It gives
/// the number of the issue.
fn comment_failure(clone: &Path, tag: &str, body: &str) -> Result<u64> {
    let title = crate::cycle::title(crate::cycle::tag_version(tag)?);
    let search = format!("\"{title}\" in:title");
    let list = run(
        clone,
        "gh",
        &[
            "issue",
            "list",
            "--state",
            "all",
            "--label",
            crate::cycle::LABEL,
            "--search",
            &search,
            "--json",
            "number,title",
        ],
    )?;
    let found: Vec<serde_json::Value> = serde_json::from_str(&list)?;
    let number = found
        .iter()
        .find(|i| i["title"] == title.as_str())
        .and_then(|i| i["number"].as_u64())
        .with_context(|| format!("no release issue \"{title}\""))?;
    let mut child = Command::new("gh")
        .args(["issue", "comment", &number.to_string(), "--body-file", "-"])
        .current_dir(clone)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .context("cannot run gh")?;
    child
        .stdin
        .take()
        .context("no stdin")?
        .write_all(body.as_bytes())?;
    let out = child.wait_with_output()?;
    if !out.status.success() {
        bail!(
            "gh issue comment failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(number)
}

/// The command of the test on the riff-server `server`: the program
/// `run` (the value of [`RUN_VAR`]), else `just real-session`. It has
/// `RIFF_SERVER` set to `server` (01M5MR8CM887EFGMKZM5BQA9JJ).
///
/// ```
/// use std::ffi::OsStr;
/// use riff::daemon::release_test::command;
///
/// let server = "https://riff.example";
/// let test = command(None, server);
/// assert_eq!(test.get_program(), "just");
/// assert_eq!(test.get_args().collect::<Vec<_>>(), ["real-session"]);
/// let env: Vec<_> = test.get_envs().collect();
/// assert_eq!(env, [(OsStr::new("RIFF_SERVER"), Some(OsStr::new(server)))]);
///
/// let fake = command(Some("/bin/fake".into()), server);
/// assert_eq!(fake.get_program(), "/bin/fake");
/// assert_eq!(fake.get_args().count(), 0);
/// ```
pub fn command(run: Option<std::ffi::OsString>, server: &str) -> Command {
    let mut command = match run {
        Some(program) => Command::new(program),
        None => {
            let mut command = Command::new("just");
            command.arg("real-session");
            command
        }
    };
    command.env("RIFF_SERVER", server);
    command
}

fn run_test(clone: &Path, log: &Path, server: &str) -> Result<bool> {
    let file = std::fs::File::create(log)?;
    let status = command(std::env::var_os(RUN_VAR), server)
        .current_dir(clone)
        .stdin(Stdio::null())
        .stdout(file.try_clone()?)
        .stderr(file)
        .status()
        .context("cannot start the test")?;
    Ok(status.success())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 01M5QVBTXE8YRY44A1C1VP71QS: the status shows the tag, the commit
    /// and the seconds of the test.
    #[test]
    fn a_started_test_shows_its_tag_its_commit_and_its_seconds() {
        let started = Started {
            tag: "v4.1.1".into(),
            commit: "90e36bfd".into(),
            at: Instant::now() - std::time::Duration::from_secs(185),
        };
        let test = started.shown();
        assert_eq!(
            (test.tag.as_str(), test.commit.as_str()),
            ("v4.1.1", "90e36bfd")
        );
        assert!((185..190).contains(&test.secs), "{}", test.secs);
    }

    /// 01M5MR8CM887EFGMKZM5BQA9JJ: the test runs with `RIFF_SERVER` of
    /// the repository, not the one of the daemon.
    #[test]
    fn the_test_gets_the_riff_server_of_the_repository() {
        let tmp = tempfile::tempdir().unwrap();
        let seen = tmp.path().join("seen");
        let program = isolated::write_program(
            tmp.path().join("run"),
            format!(
                "#!/bin/sh\nprintf %s \"$RIFF_SERVER\" > {}\n",
                seen.display()
            ),
        );
        let server = "https://riff-server.example";
        let status = command(Some(program.into()), server)
            .current_dir(tmp.path())
            .status()
            .unwrap();
        assert!(status.success());
        assert_eq!(std::fs::read_to_string(&seen).unwrap(), server);
    }
}
