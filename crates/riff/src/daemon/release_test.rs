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
//! item that the test makes. It refuses the request when the merged riff
//! config of the main clone, after a fetch, has no table `[release]`
//! ([`crate::release_config::require_merged`], 01M5SMPX8NM75P8E38DGQ17W7B,
//! 01M5TKEH5E2ZQ22PAN9FTT27VW).
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
//! `riff daemon status`, `riff top`, `riff who` and the TUI show it,
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
//!
//! The person on the host works while the test runs. When a program of
//! the person writes in the home, the lead leaves the path out of the
//! home check with `riff release test --skip PATH`
//! (01M78TS03FK4908GVSTFH1DKYY). riff sends each path relative to the
//! home of the caller ([`skip_path`]), so `~/.mozilla` in the container
//! of the lead names `~/.mozilla` of the host. The request holds the
//! paths, and the daemon gives each one to `just real-session` as
//! `--skip PATH`. The test puts a relative path in the home of the host.
//!
//! ```mermaid
//! flowchart LR
//!     L["riff release test --skip ~/.mozilla<br/>in the container of the lead"] -- "release test COMMIT TAG --skip .mozilla" --> D["riff daemon on HOST"]
//!     D -- "just real-session --skip .mozilla" --> T["the test: ~/.mozilla of HOST"]
//! ```

use std::ops::Range;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::Instant;

use anyhow::{Context, Result, bail};
use riff_client::daemons::Test;

use super::remote::Served;
use crate::cycle::{TestState, status_args};
use crate::pr::Gh;
use crate::run::hide;
use crate::text;
use crate::watches::{Op, Watches};

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

/// The `gh` of the test for the repository `repo`: it counts its calls as
/// `release test` ([`crate::ghcalls::DAEMON`]).
fn gh_of(repo: &str) -> Gh {
    Gh::for_repo(repo).by("release test")
}

fn set_status(gh: &Gh, commit: &str, state: TestState, why: &str) -> Result<()> {
    let args = status_args(commit, state, why);
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    gh.run(&args, None).map(drop)
}

/// The path of `riff release test --skip PATH` for the request
/// (01M78TS03FK4908GVSTFH1DKYY): a path in `home`, the home of the
/// caller, goes relative to the home, so it names the same path in the
/// home of the host. `~` and `~/` name the home too. Each other path
/// goes as it is. An error for an empty path and a path with a space: a
/// request is words.
///
/// ```
/// use std::path::Path;
/// use riff::daemon::release_test::skip_path;
///
/// let home = Some(Path::new("/home/claude"));
/// assert_eq!(skip_path("/home/claude/.mozilla", home).as_deref(), Ok(".mozilla"));
/// assert_eq!(skip_path("~/.config/gcloud", home).as_deref(), Ok(".config/gcloud"));
/// assert_eq!(skip_path(".cargo/bin", home).as_deref(), Ok(".cargo/bin"));
/// assert_eq!(skip_path("/home/mike/.mozilla", home).as_deref(), Ok("/home/mike/.mozilla"));
/// assert_eq!(skip_path("/home/claudette/x", home).as_deref(), Ok("/home/claudette/x"));
/// assert_eq!(skip_path("/home/claude", home).as_deref(), Ok("."));
/// assert_eq!(skip_path("/home/claude/.mozilla", None).as_deref(), Ok("/home/claude/.mozilla"));
/// assert!(skip_path("", home).is_err());
/// assert!(skip_path("~/My Files", home).unwrap_err().contains("a space"));
/// ```
pub fn skip_path(path: &str, home: Option<&Path>) -> Result<String, String> {
    if path.is_empty() {
        return Err(text::RELEASE_TEST_EMPTY_SKIP.to_owned());
    }
    if path.chars().any(char::is_whitespace) {
        return Err(text::RELEASE_TEST_SPACED_SKIP.to_owned());
    }
    let rest = match Path::new(path).strip_prefix("~") {
        Ok(rest) => Some(rest),
        Err(_) => home.and_then(|home| Path::new(path).strip_prefix(home).ok()),
    };
    Ok(match rest {
        Some(rest) if rest.as_os_str().is_empty() => ".".to_owned(),
        Some(rest) => rest.to_string_lossy().into_owned(),
        None => path.to_owned(),
    })
}

/// [`skip_path`] in the home of this process (`HOME`): the parser of
/// `riff release test --skip`.
pub fn skip_arg(path: &str) -> Result<String, String> {
    let home = std::env::var_os("HOME").filter(|home| !home.is_empty());
    skip_path(path, home.as_deref().map(Path::new))
}

/// Starts the test on `commit` in the folder `dir` of the daemon: a
/// clone of the `origin` of the main clone of `served`, detached at the
/// commit, and the status `pending`. The run goes on in a thread of its
/// own, with `--skip PATH` for each of `skips`
/// (01M78TS03FK4908GVSTFH1DKYY). It gives the line for the reply.
pub fn start(
    dir: &Path,
    served: &Served,
    commit: &str,
    tag: &str,
    skips: &[String],
) -> Result<String> {
    if !is_commit(commit) {
        bail!("\"{commit}\" is not a commit ID");
    }
    crate::release_config::require_merged(&served.wave.main)?;
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
    let gh = gh_of(&served.wave.repo);
    let started = prepare(dir, served, &gh, commit);
    match started {
        Ok(clone) => {
            // The status is `pending`: the watch reads the end of the test
            // there (01M5RGCW5NCKQYA61RM63914TX).
            let op = Op::Test {
                tag: tag.clone(),
                commit: commit.to_owned(),
            };
            let watch = Watches::new(dir)
                .register(&served.wave.repo, op.clone(), crate::daemon::now_ms())
                .map(|registered| text::watch_registered(&op.what(), registered))
                .unwrap_or_else(|e| text::watch_not_registered(&op.what(), &format!("{e:#}")));
            let owned = commit.to_owned();
            let log = clone.with_extension("log");
            let server = served.wave.server.clone();
            let skips = skips.to_vec();
            std::thread::spawn(move || {
                finish(&gh, &clone, &log, (&owned, &tag), (&server, &skips));
                *running_lock() = None;
            });
            Ok(format!(
                "The real-session test started on {commit}. It sets the status {} of the \
                 commit. {watch} next: riff release status",
                crate::cycle::REAL_SESSION
            ))
        }
        Err(e) => {
            *running_lock() = None;
            Err(e)
        }
    }
}

fn prepare(dir: &Path, served: &Served, gh: &Gh, commit: &str) -> Result<PathBuf> {
    let folder = dir.join(FOLDER);
    std::fs::create_dir_all(&folder)?;
    let clone = folder.join(commit);
    if clone.exists() {
        std::fs::remove_dir_all(&clone)?;
    }
    let origin = crate::host_git::output(&served.wave.main, &["remote", "get-url", "origin"])?;
    let clone_text = clone.to_string_lossy().into_owned();
    let args = ["clone", "-q", "--no-checkout", &origin, &clone_text];
    crate::git_credential::keyring_open(&folder, &origin, &args)?;
    crate::host_git::output(&folder, &args)?;
    crate::host_git::output(&clone, &["checkout", "-q", "--detach", commit])?;
    let host = &served.wave.host;
    set_status(
        gh,
        commit,
        TestState::Running,
        &format!("running on {host}"),
    )?;
    Ok(clone)
}

/// Runs the test in `clone` on the riff-server `server`, with `skips`
/// for its home check, then sets the final status and removes the
/// clone. A fail goes on the release issue of the tag with the failed
/// step and the end of the log, and its step is the description of the
/// status (#1255).
fn finish(
    gh: &Gh,
    clone: &Path,
    log: &Path,
    (commit, tag): (&str, &str),
    (server, skips): (&str, &[String]),
) {
    let (state, why) = match run_test(clone, log, server, skips) {
        Ok(true) => (TestState::Passed, "passed".to_owned()),
        Ok(false) => {
            let text = std::fs::read_to_string(log).unwrap_or_default();
            let step = failed_step(&text)
                .unwrap_or_else(|| "the test ended with a failure, see the comment".to_owned());
            let comment = fail_comment(tag, commit, &step, &text, TAIL_LINES);
            if let Err(e) = comment_failure(gh, tag, &comment) {
                eprintln!("riff daemon: cannot write the fail of the test on the issue: {e:#}");
            }
            (TestState::Failed, format!("FAIL at {}", redact(&step)))
        }
        Err(e) => {
            let step = format!("{e:#}");
            let comment = fail_comment(tag, commit, &step, "", TAIL_LINES);
            if let Err(e) = comment_failure(gh, tag, &comment) {
                eprintln!("riff daemon: cannot write the fail of the test on the issue: {e:#}");
            }
            (TestState::Failed, format!("FAIL: {}", redact(&step)))
        }
    };
    if let Err(e) = set_status(gh, commit, state, &why) {
        eprintln!("riff daemon: cannot set the status of the real-session test: {e:#}");
    }
    let _ = std::fs::remove_dir_all(clone);
}

/// How many last lines of the log the comment of a fail holds.
const TAIL_LINES: usize = 30;

/// `text` with each secret cut out: [`hide`] of [`redacted`], on each
/// line. The white space stays, also at the end of the line.
///
/// ```
/// use riff::daemon::release_test::redact;
///
/// assert_eq!(redact("push ghp_abc123 now"), "push [secret] now");
/// assert_eq!(redact("a key sk-proj-abc1 here"), "a key [secret] here");
/// assert_eq!(redact("-e GH_TOKEN=abc -e B=c"), "-e GH_TOKEN=[secret]");
/// assert_eq!(redact("Authorization: Bearer xyz"), "Authorization: Bearer [secret]");
/// assert_eq!(
///     redact(r#"{"msg":"x y","env":"DB_PASSWORD=a b"} x"#),
///     r#"{"msg":"x y","env":"DB_PASSWORD=[secret]"#
/// );
/// assert_eq!(redact("step 6 of 10"), "step 6 of 10");
/// ```
pub fn redact(text: &str) -> String {
    let lines: Vec<String> = text
        .lines()
        .map(|line| hide(line, redacted(line)))
        .collect();
    lines.join("\n")
}

/// The range of each secret in `text` that [`redact`] cuts out, on each
/// line (01M7GGCT0Z0GXM3BCAAGX7CE6Q): each word that starts like a token
/// of GitHub or with `sk-`, whole, and the rest of the line from
/// [`rest_from`]. It parses no quotes and no escapes.
///
/// ```
/// use riff::daemon::release_test::redacted;
///
/// assert_eq!(redacted("ghp_a1 GH_TOKEN=abc ok"), [0..6, 16..22]);
/// assert_eq!(redacted("key sk-proj-a1,API_KEY=x ok"), [4..24, 23..27]);
/// assert_eq!(redacted("Bearer xyz\nBearer\nabc ghp_a1"), [7..10, 22..28]);
/// ```
pub fn redacted(text: &str) -> Vec<Range<usize>> {
    // `sk-` is the start of each key of Anthropic.
    let token = |word: &str| {
        crate::run::TOKEN_STARTS
            .iter()
            .chain(&["sk-"])
            .any(|start| word.starts_with(start))
    };
    let mut found = Vec::new();
    let mut at = 0;
    for line in text.split_inclusive('\n') {
        let shown = &line[..line.trim_end().len()];
        let rest = rest_from(shown);
        let mut end = 0;
        while let Some(skip) = shown[end..].find(|c: char| !c.is_whitespace()) {
            let start = end + skip;
            // A word after the start of the rest is in the rest.
            if rest.is_some_and(|rest| start >= rest) {
                break;
            }
            let word = &shown[start..];
            let len = word.find(char::is_whitespace).unwrap_or(word.len());
            if token(word) {
                found.push(at + start..at + start + len);
            }
            end = start + len;
        }
        if let Some(rest) = rest {
            found.push(at + rest..at + shown.len());
        }
        at += line.len();
    }
    found
}

/// The start of the rest of `line` that [`redacted`] hides: after the
/// first of [`after_a_secret_name`] and [`after_bearer`], and after the
/// white space just after it. `None` when `line` has neither, or only
/// white space after it.
///
/// ```
/// use riff::daemon::release_test::rest_from;
///
/// assert_eq!(rest_from("a GH_TOKEN=x Bearer y"), Some(11));
/// assert_eq!(rest_from("a Bearer  y GH_TOKEN=x"), Some(10));
/// assert_eq!(rest_from("GH_TOKEN= x"), Some(10));
/// assert_eq!(rest_from("GH_TOKEN=  "), None);
/// assert_eq!(rest_from("step 6 of 10"), None);
/// ```
pub fn rest_from(line: &str) -> Option<usize> {
    let after = [after_a_secret_name(line), after_bearer(line)]
        .into_iter()
        .flatten()
        .min()?;
    let skip = line[after..].find(|c: char| !c.is_whitespace())?;
    Some(after + skip)
}

/// The end of the `=` of the first `NAME=` in `line` whose NAME names a
/// secret: it has the text TOKEN, KEY, SECRET or PASSWORD, in any place
/// and in any case (01M7GCP11AAPM078B2CXTDH03K). NAME is the run of
/// letters, also of more than one byte, digits and `_` just before the
/// `=`, at any place in the line: also after a quote, a `,`, a `&` or
/// another `=`.
///
/// ```
/// use riff::daemon::release_test::after_a_secret_name;
///
/// assert_eq!(after_a_secret_name("GH_TOKEN=abc"), Some(9));
/// assert_eq!(after_a_secret_name("GH_TOKEN_ÉQUIPE=abc"), Some(17));
/// assert_eq!(after_a_secret_name("--env=DB_PASSWORD=x"), Some(18));
/// assert_eq!(after_a_secret_name(r#"{"env":"db_password=x"}"#), Some(20));
/// assert_eq!(after_a_secret_name("?user=me&access_token=x"), Some(22));
/// assert_eq!(after_a_secret_name("SECRET_KEY_BASE=8f2k"), Some(16));
/// assert_eq!(after_a_secret_name("RIFF_ITEM=issue-12 key = x"), None);
/// ```
pub fn after_a_secret_name(line: &str) -> Option<usize> {
    const NAMES: [&str; 4] = ["TOKEN", "KEY", "SECRET", "PASSWORD"];
    line.match_indices('=').find_map(|(at, _)| {
        let before = &line[..at];
        // The run stops at the `=` before it, so each character of the
        // line is read once for the runs.
        let start = before
            .char_indices()
            .rev()
            .find(|&(_, c)| !(c.is_alphanumeric() || c == '_'))
            .map_or(0, |(i, c)| i + c.len_utf8());
        let name = before[start..].to_ascii_uppercase();
        NAMES.iter().any(|n| name.contains(n)).then_some(at + 1)
    })
}

/// The end of the first word `Bearer` in `line`, in any case, with no
/// letter or digit just before or after it: after a space, a quote, a
/// colon or a `_`, and before a space, a quote, a colon or a `_`, as in
/// `Bearer xyz`, `{"auth":"Bearer xyz"}`, `Authorization:Bearer:xyz` or
/// `Bearer_xyz`.
///
/// ```
/// use riff::daemon::release_test::after_bearer;
///
/// assert_eq!(after_bearer("Authorization: Bearer xyz"), Some(21));
/// assert_eq!(after_bearer(r#"{"auth":"bearer xyz"}"#), Some(15));
/// assert_eq!(after_bearer("Authorization:BEARER:xyz"), Some(20));
/// assert_eq!(after_bearer("my_Bearer_xyz"), Some(9));
/// assert_eq!(after_bearer("a Bearers Bearer"), Some(16));
/// for line in ["Bearers", "MyBearer", "Bear", "érBearer", "Bearer2"] {
///     assert_eq!(after_bearer(line), None, "{line}");
/// }
/// ```
pub fn after_bearer(line: &str) -> Option<usize> {
    const BEARER: &str = "bearer";
    let word = |c: Option<char>| c.is_some_and(char::is_alphanumeric);
    // The lower case keeps the place of each byte.
    let lower = line.to_ascii_lowercase();
    let at = lower.match_indices(BEARER).map(|(at, _)| at).find(|&at| {
        !word(line[..at].chars().next_back()) && !word(line[at + BEARER.len()..].chars().next())
    })?;
    Some(at + BEARER.len())
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
fn comment_failure(gh: &Gh, tag: &str, body: &str) -> Result<u64> {
    let title = crate::cycle::title(crate::cycle::tag_version(tag)?);
    let search = format!("\"{title}\" in:title");
    let list = gh.run(
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
        None,
    )?;
    let found: Vec<serde_json::Value> = serde_json::from_str(&list)?;
    let number = found
        .iter()
        .find(|i| i["title"] == title.as_str())
        .and_then(|i| i["number"].as_u64())
        .with_context(|| format!("no release issue \"{title}\""))?;
    gh.run(
        &["issue", "comment", &number.to_string(), "--body-file", "-"],
        Some(body),
    )?;
    Ok(number)
}

/// The command of the test on the riff-server `server`: the program
/// `run` (the value of [`RUN_VAR`]), else `just real-session`, with
/// `--skip PATH` for each of `skips` (01M78TS03FK4908GVSTFH1DKYY). It has
/// `RIFF_SERVER` set to `server` (01M5MR8CM887EFGMKZM5BQA9JJ).
///
/// ```
/// use std::ffi::OsStr;
/// use riff::daemon::release_test::command;
///
/// let server = "https://riff.example";
/// let test = command(None, server, &[]);
/// assert_eq!(test.get_program(), "just");
/// assert_eq!(test.get_args().collect::<Vec<_>>(), ["real-session"]);
/// let env: Vec<_> = test.get_envs().collect();
/// assert_eq!(env, [(OsStr::new("RIFF_SERVER"), Some(OsStr::new(server)))]);
///
/// let skips = [".mozilla".to_owned(), "src/other".to_owned()];
/// let test = command(None, server, &skips);
/// assert_eq!(
///     test.get_args().collect::<Vec<_>>(),
///     ["real-session", "--skip", ".mozilla", "--skip", "src/other"]
/// );
///
/// let fake = command(Some("/bin/fake".into()), server, &[]);
/// assert_eq!(fake.get_program(), "/bin/fake");
/// assert_eq!(fake.get_args().count(), 0);
/// ```
pub fn command(run: Option<std::ffi::OsString>, server: &str, skips: &[String]) -> Command {
    let mut command = match run {
        Some(program) => Command::new(program),
        None => {
            let mut command = Command::new("just");
            command.arg("real-session");
            command
        }
    };
    for skip in skips {
        command.args(["--skip", skip]);
    }
    command.env("RIFF_SERVER", server);
    command
}

fn run_test(clone: &Path, log: &Path, server: &str, skips: &[String]) -> Result<bool> {
    let file = std::fs::File::create(log)?;
    let status = command(std::env::var_os(RUN_VAR), server, skips)
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

    /// #2156, 01M7EX1Q1F468NWGPJJS0W0ZG9, 01M7GGCT0Z0GXM3BCAAGX7CE6Q: a
    /// word that starts as a token of GitHub or with `sk-`, also with no
    /// `sk-ant-`, goes whole, also with a `NAME=` in it. A word with `sk-`
    /// in it, not at its start, stays.
    #[test]
    fn a_word_with_the_start_of_a_token_goes() {
        for start in crate::run::TOKEN_STARTS.iter().chain(&["sk-ant-", "sk-"]) {
            let line = format!("a key {start}Ab1_c-9 here");
            assert_eq!(redact(&line), "a key [secret] here", "{line}");
        }
        assert_eq!(redact("a disk-1 and task-2"), "a disk-1 and task-2");
        assert_eq!(redact("key sk-proj-qw1,API_KEY=x ok"), "key [secret]");
    }

    /// #2156, 01M7GGCT0Z0GXM3BCAAGX7CE6Q: after the `=` of a NAME that
    /// names a token, a key, a secret or a password, in any case, the rest
    /// of the line goes. The white space just after the `=` stays. A
    /// `NAME=` with nothing after it hides nothing.
    #[test]
    fn the_rest_of_the_line_after_each_secret_name_goes_in_any_case() {
        for name in ["GH_TOKEN", "OPENAI_API_KEY", "CLIENT_SECRET", "DB_PASSWORD"] {
            for name in [
                name.to_owned(),
                name.to_lowercase(),
                format!("--{}", name.to_lowercase()),
            ] {
                let line = format!("-e {name}=abc1 -e B=c");
                assert_eq!(after_a_secret_name(&line), Some(name.len() + 4), "{line}");
                assert_eq!(redact(&line), format!("-e {name}=[secret]"));
            }
        }
        assert_eq!(redact("-e API_KEY=c2VjcmV0== x"), "-e API_KEY=[secret]");
        assert_eq!(redact("-e GH_TOKEN= x y"), "-e GH_TOKEN= [secret]");
        assert_eq!(redact("-e GH_TOKEN="), "-e GH_TOKEN=");
        assert_eq!(redact("-e GH_TOKEN= \t"), "-e GH_TOKEN= \t");
        assert_eq!(redact("RIFF_ITEM=issue-12 ok"), "RIFF_ITEM=issue-12 ok");
    }

    /// #2156, 01M7GCP11AAPM078B2CXTDH03K: NAME names a secret with the
    /// text at its start, in its middle or at its end, in any case, also
    /// in a longer word. A NAME with none of the texts names no secret.
    #[test]
    fn a_name_with_the_text_in_any_place_names_a_secret() {
        for name in [
            "SECRET_KEY_BASE",
            "AWS_ACCESS_KEY_ID",
            "GITHUB_TOKEN_RO",
            "PASSWORD_1",
            "my_Secret_value",
            "MONKEY",
            "TOKEN",
        ] {
            let word = format!("{name}=hunter2");
            assert_eq!(after_a_secret_name(&word), Some(name.len() + 1), "{word}");
            assert_eq!(
                redact(&format!("-e {word} ok")),
                format!("-e {name}=[secret]")
            );
        }
        for word in ["TOKE=2", "PASS_WORD=3", "K_E_Y=4", "KEY-ID=5"] {
            assert_eq!(after_a_secret_name(word), None, "{word}");
        }
    }

    /// #2156, 01M7GGCT0Z0GXM3BCAAGX7CE6Q: a letter of more than one byte
    /// is a letter of NAME: between the text of the secret and the `=`,
    /// just before the `=`, and before the text.
    #[test]
    fn a_name_with_a_wide_letter_in_it_names_a_secret() {
        for (line, shown) in [
            (
                "-e GH_TOKEN_ÉQUIPE=hunter2 x",
                "-e GH_TOKEN_ÉQUIPE=[secret]",
            ),
            ("DB_PASSWORDé=hunter2", "DB_PASSWORDé=[secret]"),
            ("a clé_KEY=hunter2 b", "a clé_KEY=[secret]"),
        ] {
            assert_eq!(redact(line), shown, "{line}");
        }
    }

    /// #2156, 01M7GGCT0Z0GXM3BCAAGX7CE6Q: NAME is at any place in the line:
    /// after a quote, a `,`, a `;`, a `&`, a `?`, a space, another `=`, or
    /// a character of more than one byte that is no letter, also when an
    /// earlier quote of the line has a space in it. The rest of the line
    /// goes, with its quotes.
    #[test]
    fn a_secret_name_at_any_place_in_the_line_hides_the_rest_of_it() {
        for (line, shown) in [
            (
                r#"{"msg":"x y","env":"DB_PASSWORD=qi1 qi2"} x"#,
                r#"{"msg":"x y","env":"DB_PASSWORD=[secret]"#,
            ),
            (
                r#"["podman","run","--name","riff one","-e","DB_PASSWORD=qj1 qj2"]"#,
                r#"["podman","run","--name","riff one","-e","DB_PASSWORD=[secret]"#,
            ),
            (
                r#"env FOO="a b",DB_PASSWORD="ql1 ql2" x"#,
                r#"env FOO="a b",DB_PASSWORD=[secret]"#,
            ),
            (
                r#"sh -c "env DB_PASSWORD=qa1 qa2" x"#,
                r#"sh -c "env DB_PASSWORD=[secret]"#,
            ),
            (
                "podman run --env=DB_PASSWORD=qe1 x",
                "podman run --env=DB_PASSWORD=[secret]",
            ),
            (r#"x=1,DB_PASSWORD="qd1 qd2" e"#, "x=1,DB_PASSWORD=[secret]"),
            (
                "grant_type=refresh_token&refresh_token=x",
                "grant_type=refresh_token&refresh_token=[secret]",
            ),
            (
                "https://h/?user=me&access_token=x",
                "https://h/?user=me&access_token=[secret]",
            ),
            ("a;API_KEY=x b", "a;API_KEY=[secret]"),
            ("'GH_TOKEN=a b' c", "'GH_TOKEN=[secret]"),
            ("?secret=a", "?secret=[secret]"),
            (
                r#"{"env":"DB_PASSWORD=a b"} x"#,
                r#"{"env":"DB_PASSWORD=[secret]"#,
            ),
            ("a\u{a0}GH_TOKEN=x y", "a\u{a0}GH_TOKEN=[secret]"),
            ("a→DB_PASSWORD=x", "a→DB_PASSWORD=[secret]"),
        ] {
            assert_eq!(redact(line), shown, "{line}");
        }
    }

    /// #2156, 01M7GGCT0Z0GXM3BCAAGX7CE6Q: no quote, no escape and no white
    /// space ends the rest of the line that goes: a no-break space, a
    /// value that starts with a space, two levels of escape, a `\` before
    /// a space, a `'` in a `"` quote, a quote with no end, and a value
    /// with a wide character. The white space at the end of the line
    /// stays, and the next line has its own rules.
    #[test]
    fn no_quote_and_no_escape_ends_the_rest_that_goes() {
        for (line, shown) in [
            ("DB_PASSWORD=qq1\u{a0}qq2 x", "DB_PASSWORD=[secret]"),
            ("DB_PASSWORD=qq1\u{3000}qq2 x", "DB_PASSWORD=[secret]"),
            (r#""GH_TOKEN= qg1 qg2" x"#, r#""GH_TOKEN= [secret]"#),
            (
                r#"{"msg":"podman run -e DB_PASSWORD=\"qn1 \\\" qn2\" next"}"#,
                r#"{"msg":"podman run -e DB_PASSWORD=[secret]"#,
            ),
            (r"DB_PASSWORD=qo1\\ qo2 x", "DB_PASSWORD=[secret]"),
            (r"GH_TOKEN='qp1 '\''qp2 qp3' x", "GH_TOKEN=[secret]"),
            (r#"DB_PASSWORD="it's a secret" x"#, "DB_PASSWORD=[secret]"),
            (r#"DB_PASSWORD="a b c"#, "DB_PASSWORD=[secret]"),
            ("GH_TOKEN=abé x", "GH_TOKEN=[secret]"),
            ("GH_TOKEN='a b \t", "GH_TOKEN=[secret] \t"),
        ] {
            assert_eq!(redact(line), shown, "{line:?}");
        }
        assert_eq!(
            redact("GH_TOKEN=\"a b\nnext line\" x"),
            "GH_TOKEN=[secret]\nnext line\" x"
        );
    }

    /// #2156, 01M7GGCT0Z0GXM3BCAAGX7CE6Q: the rest of the line after the
    /// word `Bearer` goes in each form of a header: a plain one, in JSON,
    /// in quotes, with no space after the colon, with two spaces, with a
    /// tab, with a no-break space, with a `"`, a `'` or a `:` after
    /// `Bearer`, in single quotes, and after a letter whose lower case has
    /// more bytes. The white space just after `Bearer` stays.
    #[test]
    fn the_rest_of_the_line_after_bearer_goes_in_each_form_of_a_header() {
        for (line, shown) in [
            (
                "Authorization: Bearer plain-form-1",
                "Authorization: Bearer [secret]",
            ),
            (
                r#"{"Authorization":"Bearer json-form-2"}"#,
                r#"{"Authorization":"Bearer [secret]"#,
            ),
            (
                r#"Authorization: "Bearer quoted-form-3""#,
                r#"Authorization: "Bearer [secret]"#,
            ),
            (
                "Authorization:Bearer nospace-form-4",
                "Authorization:Bearer [secret]",
            ),
            (
                "Authorization: Bearer  twospace-form-5 end",
                "Authorization: Bearer  [secret]",
            ),
            (
                "Authorization: bearer\ttab-form-6",
                "Authorization: bearer\t[secret]",
            ),
            ("Bearer\u{a0}nbsp-form-7 x", "Bearer\u{a0}[secret]"),
            (r#""Bearer" quoted-end-8"#, r#""Bearer[secret]"#),
            ("'Bearer' single-end-9", "'Bearer[secret]"),
            ("Bearer 'single-form-10'", "Bearer [secret]"),
            (
                "Authorization: Bearer: colon-end-11",
                "Authorization: Bearer[secret]",
            ),
            ("Authorization: Bearer:xyz", "Authorization: Bearer[secret]"),
            (r#"{"Bearer": "json-key-12"}"#, r#"{"Bearer[secret]"#),
            (
                r#""Authorization": "Bearer tok", "x": 1"#,
                r#""Authorization": "Bearer [secret]"#,
            ),
            (
                r#"{"token_type":"Bearer","access_token":"xyz"}"#,
                r#"{"token_type":"Bearer[secret]"#,
            ),
            (
                r#"header("Authorization", "Bearer abc:def")"#,
                r#"header("Authorization", "Bearer [secret]"#,
            ),
            (r#"{"msg":"Bearer \"abc\""}"#, r#"{"msg":"Bearer [secret]"#),
            (
                r#"{"Authorization":"Bearer "} refused"#,
                r#"{"Authorization":"Bearer [secret]"#,
            ),
            ("a Bearer xyz Bearer abc end", "a Bearer [secret]"),
            ("ghp_abc1 Bearer xyz", "[secret] Bearer [secret]"),
            ("İ Bearer xyz", "İ Bearer [secret]"),
        ] {
            assert_eq!(redact(line), shown, "{line:?}");
        }
        for line in ["Bearer", "Bearer ", "a Bearers b", "Bearer\nnext line"] {
            assert_eq!(redact(line), line, "{line:?}");
        }
    }

    /// #2156, 01M7GGCT0Z0GXM3BCAAGX7CE6Q: the rest of the line goes from
    /// the first of a secret name and `Bearer`, in each order.
    #[test]
    fn the_rest_goes_from_the_first_of_a_secret_name_and_bearer() {
        for (line, shown) in [
            ("a GH_TOKEN=x Bearer y", "a GH_TOKEN=[secret]"),
            ("a Bearer y GH_TOKEN=x", "a Bearer [secret]"),
        ] {
            assert_eq!(redact(line), shown, "{line}");
        }
    }

    /// #2156, 01M7GGCT0Z0GXM3BCAAGX7CE6Q: the word `Bearer` next to a
    /// letter of more than one byte is no word `Bearer`, and hides
    /// nothing. A word with a wide letter where `Bearer` could end stays.
    #[test]
    fn a_bearer_next_to_a_wide_letter_hides_nothing() {
        for line in ["éBearer x", "Beareré x", "a Résumé here"] {
            assert_eq!(redact(line), line, "{line}");
        }
    }

    /// #2156, 01M7GGCT0Z0GXM3BCAAGX7CE6Q: a `_` is no letter and no
    /// digit. So `Bearer` with a `_` just before or after it is the word
    /// `Bearer`, and the rest of the line after it goes, with the `_`.
    #[test]
    fn a_bearer_next_to_an_underscore_hides_the_rest_of_the_line() {
        for (line, shown) in [
            ("Authorization: Bearer_xyz", "Authorization: Bearer[secret]"),
            ("auth=Bearer_tok-1", "auth=Bearer[secret]"),
            ("my_bearer xyz b", "my_bearer [secret]"),
            ("_BEARER_ xyz", "_BEARER[secret]"),
        ] {
            assert_eq!(redact(line), shown, "{line}");
        }
    }

    /// #2156, 01M7GGCT0Z0GXM3BCAAGX7CE6Q: the white space at the end of a
    /// line stays, after a word, after the rest that goes, and in a line
    /// of white space only.
    #[test]
    fn the_white_space_at_the_end_of_a_line_stays() {
        for (line, shown) in [
            ("a word ", "a word "),
            ("a word \t", "a word \t"),
            ("Bearer xyz a ", "Bearer [secret] "),
            ("Bearer xyz\t \t", "Bearer [secret]\t \t"),
            ("GH_TOKEN=a b\u{a0}", "GH_TOKEN=[secret]\u{a0}"),
            ("push ghp_abc123  ", "push [secret]  "),
            ("   ", "   "),
            ("\t", "\t"),
        ] {
            assert_eq!(redact(line), shown, "{line:?}");
        }
        assert_eq!(redact("Bearer xyz \nnext "), "Bearer [secret] \nnext ");
    }

    /// #2156, 01M7FXE86QYC8D6P6WDK69ZSMA, 01M7GGCT0Z0GXM3BCAAGX7CE6Q: the
    /// range of a secret on a later line of the text counts each byte of
    /// the lines before it, also of a wide character and of `\r\n`. A
    /// `Bearer` at the end of a line hides nothing on the next line.
    #[test]
    fn a_secret_on_a_later_line_has_its_range_in_the_whole_text() {
        let text = "é ok\r\nGH_TOKEN=abc\nBearer\nnext ghp_a1 Bearer x";
        let found = redacted(text);
        assert_eq!(found, [16..19, 32..38, 46..47]);
        let parts: Vec<&str> = found.iter().map(|r| &text[r.clone()]).collect();
        assert_eq!(parts, ["abc", "ghp_a1", "x"]);
        assert_eq!(
            hide(text, found),
            "é ok\r\nGH_TOKEN=[secret]\nBearer\nnext [secret] Bearer [secret]"
        );
    }

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
        let status = command(Some(program.into()), server, &[])
            .current_dir(tmp.path())
            .status()
            .unwrap();
        assert!(status.success());
        assert_eq!(std::fs::read_to_string(&seen).unwrap(), server);
    }
}
