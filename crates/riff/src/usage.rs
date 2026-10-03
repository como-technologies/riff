//! The tokens of each issue, and the models that did the work (#330).
//!
//! # Design
//!
//! The agent tool writes a transcript for each session, on the machine
//! of the session. Each reply of the model has its model and its usage:
//! four kinds of tokens ([`Tokens`]). riff knows when each claim of a
//! session starts and ends. So riff sums the replies of a session from
//! the start of a claim to its end, for each model
//! (01M3Y1YP0QY11VR28RF9MKPN0G).
//!
//! The transcripts stay on their machine. So the sum of a claim goes to
//! a place that each machine reads: a comment on the issue of the claim
//! (01M3Y1YP1ZA5TBRA01MKWM3VC6). The log of `riff-server` does not
//! change.
//!
//! ```mermaid
//! sequenceDiagram
//!     participant H as start hook
//!     participant T as claim and release
//!     participant L as usage-ID.json
//!     participant X as transcripts
//!     participant G as gh
//!     H->>L: the path of the transcript
//!     T->>L: claim: the item and its start time
//!     T->>L: release: the end time
//!     L->>X: sum the replies from the start to the end
//!     T->>G: one comment on the issue, with the sum
//!     Note over G: riff usage ISSUE reads the comments and sums them
//! ```
//!
//! # The marks of a session
//!
//! `/clear` gives a session a new transcript, and the session keeps its
//! riff session ID. Only a hook sees the path of the new transcript. So
//! riff keeps one file for each session, `usage-ID.json` in
//! [`crate::local::marks`]: each transcript of the session, and each
//! claim with its times ([`Ledger`], 01M3Y1YP15C7AT2N70BWQP8PE2).
//!
//! | Who | Writes |
//! |---|---|
//! | the start hook, the Stop hook, the end hook | the path of the transcript ([`saw`]) |
//! | `claim` (the tool and the command) | the item and its start time ([`started`]) |
//! | `release`, `leave` | the end time and the sum ([`ended`], [`ended_all`]) |
//! | the start hook at a new start, the end hook | the end of each claim that is still open ([`ended_all`]) |
//!
//! # The sum
//!
//! - The agent tool writes one reply on two or three lines with the same
//!   `message.id`. The sum counts each reply one time ([`Replies`]).
//! - A helper agent has transcripts of its own, in the directory
//!   `SESSION/subagents` next to `SESSION.jsonl`. They count too.
//! - A reply counts for a claim when its time is from the start of the
//!   claim to before its end.
//! - Tokens in a time with two claims of one session count for the
//!   claim that started last ([`windows`], 01M3Y1YP1JAQMJ66K2QXC7766C).
//!   So the claims of a session never count a reply two times.
//!
//! A claim that another session freed, or that the server freed, stays
//! open in the marks until the next start or the end of its session.
//!
//! # The comment
//!
//! `issue-N` and `verify-issue-N` are claims of the issue N
//! ([`issue_of`]). The comment has one line for a person, then the
//! [`Report`] as JSON in a `details` block with the mark [`MARK`]
//! (01M3Y1YP2CSNHCWV7T4CE9HZ4Y). It holds only numbers and names: the
//! item, the kind of the claim, the first characters of the session ID,
//! the times, and the four kinds of tokens for each model. It holds no
//! text of a transcript, no path and no email.
//!
//! A second report of the same claim replaces the comment that the same
//! person wrote (01M3Y1YP2TVYQC7GCCAMN6111K). A report never fails a
//! release: with no `gh`, or with an item that names no issue, the sum
//! stays in the marks of the machine, and the text says so
//! (01M3Y1YP39VFX6GH33H7B8A8KR). A claim with no tokens gets no
//! comment.
//!
//! # `riff usage`
//!
//! | Command | Shows |
//! |---|---|
//! | `riff usage ISSUE` | The comments of the issue with the mark: the total, then each claim, with its models and who wrote its comment (01M3Y1YP3QMKS6B35PJ42KNYXX). |
//! | `riff usage --wave "Wave N"` | Each issue of the wave with its total, and the sum (01M3Y1YP45VQS5HMJCXKRN3CCR). |
//! | `riff usage` | Each session of this machine: the tokens of each item, and `no issue` for the tokens outside each claim (01M3Y1YP4KVHK1DTZ85YNGDG0T). |
//!
//! Each person who can write a comment on the issue can write one with
//! the mark. So `riff usage ISSUE` names who wrote each comment that it
//! sums, and it trusts such a comment only for its numbers
//! (01M3Y9TD41FZBDQBK42FVG89B8):
//!
//! - It takes a report only when its item is a claim of that issue and
//!   its session and its models are names ([`Report::valid`]). So a
//!   comment cannot put a line, a control character, a mention or a
//!   link into the text of `riff usage` or into the total comment.
//! - A comment replaces only an earlier comment of the same person for
//!   the same claim ([`counted`]). A comment of another person for that
//!   claim counts as a report of that person.
//! - A sum never fails on a large number: it stays at the largest
//!   number ([`Tokens::total`]).
//! - It says how many comments with the mark it did not count, and why
//!   ([`uncounted`], 01M3ZRQY9F9P7DF187Q0PJDS30).
//! - Each login and title of the forge goes through
//!   [`text::forge`](crate::text::forge) (01M3ZRQY6YQ8QAKZPGWH1XD6WW).
//!
//! After the merge, `riff pr wait` adds one comment with the total of
//! the issue ([`Forge::total`], 01M3Y1YP514MPX8DTKMTWDHE8Q). Each later
//! report of a claim of the issue writes that comment again.
//!
//! # Example
//!
//! ```
//! use riff::usage::{Replies, Tokens};
//!
//! let reply = |at: &str, id: &str, model: &str, out: u64| format!(
//!     r#"{{"type":"assistant","timestamp":"{at}","message":{{"id":"{id}","model":"{model}","usage":{{"input_tokens":1,"output_tokens":{out},"cache_creation_input_tokens":10,"cache_read_input_tokens":100}}}}}}"#
//! );
//! let transcript = [
//!     reply("2026-10-02T09:00:00.000Z", "m1", "opus", 5),
//!     // The agent tool writes a reply on more than one line.
//!     reply("2026-10-02T09:00:00.100Z", "m1", "opus", 5),
//!     reply("2026-10-02T09:00:01.000Z", "m2", "haiku", 7),
//!     reply("2026-10-02T11:00:00.000Z", "m3", "opus", 9),
//! ]
//! .join("\n");
//! let mut replies = Replies::default();
//! replies.add(&transcript);
//!
//! // 09:00:00 to 10:00:00.
//! let models = replies.sum(&[(1_790_931_600_000, 1_790_935_200_000)]);
//! let opus = Tokens { input: 1, output: 5, cache_write: 10, cache_read: 100 };
//! assert_eq!(models["opus"], opus);
//! assert_eq!(models["haiku"].output, 7);
//! assert_eq!(riff::usage::total(&models).total(), 116 + 118);
//! ```

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt::Write as _;
use std::fs::OpenOptions;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::pr::Gh;
use crate::text;

/// The mark of the comment of one claim: the summary of its `details`
/// block.
pub const MARK: &str = "riff:usage";

/// The first line of the comment with the total of an issue.
pub const TOTAL_MARK: &str = "<!-- riff:usage-total -->";

/// The characters of the session ID that a comment shows.
const ID_CHARS: usize = 8;

/// The longest name of a model in a report, in characters.
const NAME_CHARS: usize = 100;

/// The most models in a report.
const MODELS: usize = 32;

/// True for a character of a name in a report: an ASCII letter, a
/// digit, `.`, `_`, `-` or `:`.
fn name_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | ':')
}

/// True when `text` is a name of at most `most` characters.
fn is_name(text: &str, most: usize) -> bool {
    !text.is_empty() && text.len() <= most && text.chars().all(name_char)
}

/// The first `most` characters of `text` as a name: each character that
/// a name cannot hold is `-`.
fn name(text: &str, most: usize) -> String {
    let name: String = text
        .chars()
        .take(most)
        .map(|c| if name_char(c) { c } else { '-' })
        .collect();
    match name.is_empty() {
        true => "-".into(),
        false => name,
    }
}

/// The four kinds of tokens of a reply of the model. A cache read costs
/// much less than an output token, so each kind has its own count.
///
/// ```
/// use riff::usage::Tokens;
///
/// let tokens = Tokens { input: 1_200, output: 34, cache_write: 5, cache_read: 1_000_000 };
/// assert_eq!(tokens.total(), 1_001_239);
/// assert_eq!(
///     tokens.text(),
///     "1,001,239 tokens (input 1,200, output 34, cache write 5, cache read 1,000,000)"
/// );
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tokens {
    pub input: u64,
    pub output: u64,
    pub cache_write: u64,
    pub cache_read: u64,
}

impl Tokens {
    /// The sum of the four kinds. A sum past the largest number stays
    /// at the largest number: a comment of another person can hold each
    /// number, and the sum never fails.
    pub fn total(&self) -> u64 {
        self.input
            .saturating_add(self.output)
            .saturating_add(self.cache_write)
            .saturating_add(self.cache_read)
    }

    /// The total, then each kind.
    pub fn text(&self) -> String {
        format!(
            "{} tokens (input {}, output {}, cache write {}, cache read {})",
            count(self.total()),
            count(self.input),
            count(self.output),
            count(self.cache_write),
            count(self.cache_read)
        )
    }

    /// These tokens less `other`, and never less than 0.
    fn less(&self, other: &Tokens) -> Tokens {
        Tokens {
            input: self.input.saturating_sub(other.input),
            output: self.output.saturating_sub(other.output),
            cache_write: self.cache_write.saturating_sub(other.cache_write),
            cache_read: self.cache_read.saturating_sub(other.cache_read),
        }
    }
}

impl std::ops::AddAssign for Tokens {
    fn add_assign(&mut self, other: Tokens) {
        self.input = self.input.saturating_add(other.input);
        self.output = self.output.saturating_add(other.output);
        self.cache_write = self.cache_write.saturating_add(other.cache_write);
        self.cache_read = self.cache_read.saturating_add(other.cache_read);
    }
}

impl std::iter::Sum for Tokens {
    fn sum<I: Iterator<Item = Tokens>>(iter: I) -> Tokens {
        iter.fold(Tokens::default(), |mut sum, tokens| {
            sum += tokens;
            sum
        })
    }
}

/// The tokens of each model, by the name of the model.
pub type Models = BTreeMap<String, Tokens>;

/// The tokens of all the models.
pub fn total(models: &Models) -> Tokens {
    models.values().copied().sum()
}

/// A time from its first value to before its second value, in
/// milliseconds since the Unix epoch.
pub type Window = (u64, u64);

/// A number with a comma between each three digits.
///
/// ```
/// assert_eq!(riff::usage::count(0), "0");
/// assert_eq!(riff::usage::count(999), "999");
/// assert_eq!(riff::usage::count(1_234_567), "1,234,567");
/// ```
pub fn count(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, digit) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

/// The time now, in milliseconds since the Unix epoch.
pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

/// The part of one line of a transcript that the sum reads.
#[derive(Deserialize)]
struct Line {
    #[serde(default)]
    timestamp: Option<String>,
    #[serde(default)]
    message: Option<Reply>,
}

#[derive(Deserialize)]
struct Reply {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    usage: Option<Used>,
}

#[derive(Deserialize)]
struct Used {
    #[serde(default)]
    input_tokens: u64,
    #[serde(default)]
    output_tokens: u64,
    #[serde(default)]
    cache_creation_input_tokens: u64,
    #[serde(default)]
    cache_read_input_tokens: u64,
}

/// The replies of the model in the transcripts of a session, each one
/// time (01M3Y1YP0QY11VR28RF9MKPN0G).
#[derive(Debug, Default)]
pub struct Replies(HashMap<String, (u64, String, Tokens)>);

impl Replies {
    /// Adds each reply in the text of one transcript. A line that is no
    /// reply with a time, a model and a usage is skipped. A later line
    /// of the same reply replaces the earlier one.
    pub fn add(&mut self, transcript: &str) {
        for line in transcript.lines() {
            let Ok(Line {
                timestamp: Some(at),
                message: Some(reply),
            }) = serde_json::from_str::<Line>(line)
            else {
                continue;
            };
            let (Some(model), Some(used)) = (reply.model, reply.usage) else {
                continue;
            };
            let Ok(at) = chrono::DateTime::parse_from_rfc3339(&at) else {
                continue;
            };
            let at_ms = u64::try_from(at.timestamp_millis()).unwrap_or(0);
            let tokens = Tokens {
                input: used.input_tokens,
                output: used.output_tokens,
                cache_write: used.cache_creation_input_tokens,
                cache_read: used.cache_read_input_tokens,
            };
            let id = reply.id.unwrap_or_else(|| format!("{at_ms} {model}"));
            self.0.insert(id, (at_ms, model, tokens));
        }
    }

    /// The replies of each transcript of `paths`, and of the helper
    /// agents of each one. A file that cannot be read is skipped: the
    /// agent tool can remove an old transcript.
    pub fn read(paths: &[PathBuf]) -> Replies {
        let mut replies = Replies::default();
        for path in paths {
            let helpers = std::fs::read_dir(path.with_extension("").join("subagents"))
                .into_iter()
                .flatten()
                .flatten()
                .map(|entry| entry.path())
                .filter(|p| p.extension().is_some_and(|e| e == "jsonl"));
            for file in std::iter::once(path.clone()).chain(helpers) {
                if let Ok(text) = std::fs::read_to_string(&file) {
                    replies.add(&text);
                }
            }
        }
        replies
    }

    /// The tokens of each model in the replies whose time is in one of
    /// `windows`. A model with no tokens is left out.
    pub fn sum(&self, windows: &[Window]) -> Models {
        let mut models = Models::new();
        for (at_ms, model, tokens) in self.0.values() {
            let counts = windows.iter().any(|(from, to)| from <= at_ms && at_ms < to);
            if counts && tokens.total() > 0 {
                *models.entry(model.clone()).or_default() += *tokens;
            }
        }
        models
    }
}

/// One claim of a session, with its times and its sum.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Claim {
    /// The thread of the claim: `OWNER/REPO` for a repository.
    pub thread: String,
    /// The work item, for example `issue-12`.
    pub item: String,
    /// The time of the claim.
    pub from_ms: u64,
    /// The time of the end of the claim. `None` while the session holds
    /// it.
    #[serde(default)]
    pub to_ms: Option<u64>,
    /// The tokens of each model, from the end of the claim on.
    #[serde(default)]
    pub models: Models,
}

/// The marks of one session on this machine: its transcripts and its
/// claims (01M3Y1YP15C7AT2N70BWQP8PE2).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ledger {
    /// Each transcript of the session. `/clear` starts a new one.
    #[serde(default)]
    pub transcripts: Vec<PathBuf>,
    /// Each claim of the session, in the order of the claims.
    #[serde(default)]
    pub claims: Vec<Claim>,
}

impl Ledger {
    /// The sum of the claim `index` at the time `now`. An open claim
    /// counts up to `now`.
    fn sum(&self, replies: &Replies, index: usize, now: u64) -> Models {
        replies.sum(&windows(&self.claims, index, now))
    }

    /// The tokens of the session outside each claim, at the time `now`:
    /// the tokens for no issue.
    pub fn no_issue(&self, now: u64) -> Tokens {
        let replies = Replies::read(&self.transcripts);
        let claimed: Tokens = (0..self.claims.len())
            .map(|i| total(&self.sum(&replies, i, now)))
            .sum();
        total(&replies.sum(&[(0, u64::MAX)])).less(&claimed)
    }
}

/// The times that count for the claim `index` of `claims`: from its
/// start to its end, less the time of each claim that started after it
/// (01M3Y1YP1JAQMJ66K2QXC7766C). A claim that is open ends at `now`.
///
/// ```
/// use riff::usage::{Claim, windows};
///
/// let claim = |from_ms, to_ms| Claim {
///     thread: "acme/app".into(),
///     item: "issue-7".into(),
///     from_ms,
///     to_ms,
///     models: Default::default(),
/// };
/// // A work claim from 10, and a verify from 20 to 30 in the same session.
/// let claims = [claim(10, None), claim(20, Some(30))];
/// assert_eq!(windows(&claims, 0, 50), [(10, 20), (30, 50)]);
/// assert_eq!(windows(&claims, 1, 50), [(20, 30)]);
/// ```
pub fn windows(claims: &[Claim], index: usize, now: u64) -> Vec<Window> {
    let end = |claim: &Claim| claim.to_ms.unwrap_or(now);
    let me = &claims[index];
    let mut keep = vec![(me.from_ms, end(me))];
    for (i, other) in claims.iter().enumerate() {
        if (other.from_ms, i) > (me.from_ms, index) {
            let cut = (other.from_ms, end(other));
            keep = keep
                .into_iter()
                .flat_map(|(from, to)| [(from, to.min(cut.0)), (from.max(cut.1), to)])
                .filter(|(from, to)| from < to)
                .collect();
        }
    }
    keep
}

fn ledger_file(dir: &Path, session: &str) -> PathBuf {
    dir.join(format!("usage-{}.json", riff_core::name::sanitize(session)))
}

/// The marks of `session` in `dir`. With no file, or a file that does
/// not read, the session has no marks.
pub fn load(dir: &Path, session: &str) -> Ledger {
    std::fs::read_to_string(ledger_file(dir, session))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

/// Each session with marks in `dir`, in the order of their names.
pub fn sessions(dir: &Path) -> Vec<String> {
    let mut sessions: Vec<String> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            Some(
                name.strip_prefix("usage-")?
                    .strip_suffix(".json")?
                    .to_owned(),
            )
        })
        .collect();
    sessions.sort();
    sessions
}

/// Changes the marks of `session` in `dir` with `change`, under a lock:
/// a hook, `riff mcp` and a command can write at the same time.
fn change<T>(dir: &Path, session: &str, change: impl FnOnce(&mut Ledger) -> T) -> io::Result<T> {
    std::fs::create_dir_all(dir)?;
    let path = ledger_file(dir, session);
    let lock = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .open(path.with_extension("lock"))?;
    lock.lock()?;
    let mut ledger = load(dir, session);
    let out = change(&mut ledger);
    let new = path.with_extension("new");
    std::fs::write(&new, serde_json::to_string(&ledger)?)?;
    std::fs::rename(new, path)?;
    Ok(out)
}

/// Records `transcript` as a transcript of `session`.
///
/// ```
/// let marks = tempfile::tempdir()?;
/// riff::usage::saw(marks.path(), "a6cf", "/t/one.jsonl".as_ref())?;
/// riff::usage::saw(marks.path(), "a6cf", "/t/one.jsonl".as_ref())?;
/// riff::usage::saw(marks.path(), "a6cf", "/t/two.jsonl".as_ref())?;
/// assert_eq!(riff::usage::load(marks.path(), "a6cf").transcripts.len(), 2);
/// assert_eq!(riff::usage::sessions(marks.path()), ["a6cf"]);
/// # Ok::<(), std::io::Error>(())
/// ```
pub fn saw(dir: &Path, session: &str, transcript: &Path) -> io::Result<()> {
    if load(dir, session)
        .transcripts
        .iter()
        .any(|known| known == transcript)
    {
        return Ok(());
    }
    change(dir, session, |ledger| {
        if !ledger.transcripts.iter().any(|known| known == transcript) {
            ledger.transcripts.push(transcript.to_owned());
        }
    })
}

/// Records that `session` holds `item` in `thread` from `now`. A claim
/// of an item that the session holds already changes nothing.
pub fn started(dir: &Path, session: &str, thread: &str, item: &str, now: u64) -> io::Result<()> {
    change(dir, session, |ledger| {
        let open = |c: &Claim| c.thread == thread && c.item == item && c.to_ms.is_none();
        if !ledger.claims.iter().any(open) {
            ledger.claims.push(Claim {
                thread: thread.to_owned(),
                item: item.to_owned(),
                from_ms: now,
                to_ms: None,
                models: Models::new(),
            });
        }
    })
}

/// Ends the open claim of `item` in `thread` at `now`, and sums it.
/// `None` when the marks have no such claim.
///
/// ```
/// use riff::usage::{ended, load, started};
///
/// let marks = tempfile::tempdir()?;
/// assert_eq!(ended(marks.path(), "a6cf", "acme/app", "issue-7", 30)?, None);
/// started(marks.path(), "a6cf", "acme/app", "issue-7", 10)?;
/// let claim = ended(marks.path(), "a6cf", "acme/app", "issue-7", 30)?.expect("open");
/// assert_eq!((claim.from_ms, claim.to_ms), (10, Some(30)));
/// assert_eq!(load(marks.path(), "a6cf").claims, [claim]);
/// # Ok::<(), std::io::Error>(())
/// ```
pub fn ended(
    dir: &Path,
    session: &str,
    thread: &str,
    item: &str,
    now: u64,
) -> io::Result<Option<Claim>> {
    let open = |c: &Claim| c.thread == thread && c.item == item && c.to_ms.is_none();
    end(dir, session, now, open).map(|mut claims| claims.pop())
}

/// Ends each open claim of `session` that started before `now`, at
/// `now`, and sums each one. A new start and the end of a session free
/// each claim.
pub fn ended_all(dir: &Path, session: &str, now: u64) -> io::Result<Vec<Claim>> {
    if load(dir, session).claims.iter().all(|c| c.to_ms.is_some()) {
        return Ok(Vec::new());
    }
    end(dir, session, now, |c| c.to_ms.is_none() && c.from_ms < now)
}

fn end(
    dir: &Path,
    session: &str,
    now: u64,
    ends: impl Fn(&Claim) -> bool,
) -> io::Result<Vec<Claim>> {
    change(dir, session, |ledger| {
        let done: Vec<usize> = (0..ledger.claims.len())
            .filter(|&i| ends(&ledger.claims[i]))
            .collect();
        if done.is_empty() {
            return Vec::new();
        }
        for &i in &done {
            ledger.claims[i].to_ms = Some(now.max(ledger.claims[i].from_ms));
        }
        let replies = Replies::read(&ledger.transcripts);
        for &i in &done {
            ledger.claims[i].models = ledger.sum(&replies, i, now);
        }
        done.iter().map(|&i| ledger.claims[i].clone()).collect()
    })
}

/// The open claim of `item` of `session`, as if it ended at `now`. The
/// marks do not change.
pub fn so_far(dir: &Path, session: &str, item: &str, now: u64) -> Option<Claim> {
    let ledger = load(dir, session);
    let index = ledger
        .claims
        .iter()
        .position(|c| c.item == item && c.to_ms.is_none())?;
    let models = ledger.sum(&Replies::read(&ledger.transcripts), index, now);
    Some(Claim {
        to_ms: Some(now),
        models,
        ..ledger.claims[index].clone()
    })
}

/// The kind of a claim.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// The claim of the work item.
    Work,
    /// The claim of a verify of the work item.
    Verify,
}

impl Kind {
    fn word(self) -> &'static str {
        match self {
            Kind::Work => "work",
            Kind::Verify => "verify",
        }
    }
}

/// The issue and the kind of the claim of `item`. `None` for an item
/// that names no issue.
///
/// ```
/// use riff::usage::{Kind, issue_of};
///
/// assert_eq!(issue_of("issue-12"), Some((12, Kind::Work)));
/// assert_eq!(issue_of("verify-issue-12"), Some((12, Kind::Verify)));
/// assert_eq!(issue_of("docs"), None);
/// assert_eq!(issue_of("issue-12b"), None);
/// ```
pub fn issue_of(item: &str) -> Option<(u64, Kind)> {
    let (kind, rest) = match item.strip_prefix("verify-") {
        Some(rest) => (Kind::Verify, rest),
        None => (Kind::Work, item),
    };
    Some((rest.strip_prefix("issue-")?.parse().ok()?, kind))
}

/// The item of the claim of `kind` of `issue`.
fn claim_item(issue: u64, kind: Kind) -> String {
    match kind {
        Kind::Work => format!("issue-{issue}"),
        Kind::Verify => format!("verify-issue-{issue}"),
    }
}

/// The sum of one claim, as a comment on its issue holds it
/// (01M3Y1YP2CSNHCWV7T4CE9HZ4Y). It holds only numbers and names.
///
/// ```
/// use riff::usage::{Claim, Kind, Report, Tokens};
///
/// let claim = Claim {
///     thread: "acme/app".into(),
///     item: "verify-issue-7".into(),
///     from_ms: 1_790_931_600_000,
///     to_ms: Some(1_790_935_200_000),
///     models: [("opus".to_owned(), Tokens { input: 1, output: 2, cache_write: 3, cache_read: 4 })].into(),
/// };
/// let (issue, report) = Report::of(&claim, "a6cf0123-4567").expect("an issue");
/// assert_eq!((issue, report.kind, report.session.as_str()), (7, Kind::Verify, "a6cf0123"));
/// let comment = report.comment();
/// assert!(comment.starts_with(
///     "riff usage: verify-issue-7, verify, session a6cf0123, 2026-10-02 09:00 to \
///      2026-10-02 10:00 UTC: 10 tokens (input 1, output 2, cache write 3, cache read 4). \
///      Models: opus.\n"
/// ));
/// assert!(comment.contains("<details><summary>riff:usage</summary>"));
/// assert_eq!(Report::parse(&comment), Some(report));
/// assert_eq!(Report::parse("A comment of a person."), None);
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Report {
    pub item: String,
    pub kind: Kind,
    /// The first characters of the session ID.
    pub session: String,
    pub from_ms: u64,
    pub to_ms: u64,
    pub models: Models,
}

impl Report {
    /// The report of the ended `claim` of `session`, with its issue.
    /// `None` when the item names no issue, or the claim is open.
    pub fn of(claim: &Claim, session: &str) -> Option<(u64, Report)> {
        let (issue, kind) = issue_of(&claim.item)?;
        let mut models = Models::new();
        for (model, tokens) in &claim.models {
            *models.entry(name(model, NAME_CHARS)).or_default() += *tokens;
        }
        let report = Report {
            item: claim.item.clone(),
            kind,
            session: name(session, ID_CHARS),
            from_ms: claim.from_ms,
            to_ms: claim.to_ms?,
            models,
        };
        Some((issue, report))
    }

    /// True when the report is a report of a claim of `issue`, and its
    /// texts are names (01M3Y9TD41FZBDQBK42FVG89B8). Each person who can
    /// write a comment on the issue can write the JSON of a report. So
    /// `riff usage` takes a report only when its item is `issue-N` or
    /// `verify-issue-N` of that issue, its kind agrees with the item,
    /// and its session and each of its models have only the characters
    /// of a name. Then a report cannot hold a line break, a control
    /// character, a mention or a link.
    ///
    /// ```
    /// use riff::usage::{Kind, Report, Tokens};
    ///
    /// let report = Report {
    ///     item: "issue-7".into(),
    ///     kind: Kind::Work,
    ///     session: "a6cf0123".into(),
    ///     from_ms: 1,
    ///     to_ms: 2,
    ///     models: [("us.claude-opus-5-5:0".to_owned(), Tokens::default())].into(),
    /// };
    /// assert!(report.valid(7));
    /// assert!(!report.valid(8), "a report of another issue");
    /// let other = |change: fn(&mut Report)| {
    ///     let mut other = report.clone();
    ///     change(&mut other);
    ///     other
    /// };
    /// assert!(!other(|r| r.kind = Kind::Verify).valid(7));
    /// assert!(!other(|r| r.item = "issue-7, work\n  issue-7".into()).valid(7));
    /// assert!(!other(|r| r.session = "a6cf\u{1b}[2K".into()).valid(7));
    /// assert!(!other(|r| r.session = "a6cf01234".into()).valid(7));
    /// assert!(!other(|r| r.models = [("m @everyone".to_owned(), Tokens::default())].into()).valid(7));
    /// assert!(!other(|r| r.models = [("[x](http://e.test)".to_owned(), Tokens::default())].into()).valid(7));
    /// ```
    pub fn valid(&self, issue: u64) -> bool {
        issue_of(&self.item) == Some((issue, self.kind))
            && self.item == claim_item(issue, self.kind)
            && is_name(&self.session, ID_CHARS)
            && self.models.len() <= MODELS
            && self.models.keys().all(|model| is_name(model, NAME_CHARS))
    }

    /// The line for a person: the claim, its times and its tokens.
    pub fn line(&self) -> String {
        let models: Vec<&str> = self.models.keys().map(String::as_str).collect();
        let models = match models.is_empty() {
            true => String::new(),
            false => format!(" Models: {}.", models.join(", ")),
        };
        format!(
            "{}, {}, session {}, {} to {} UTC: {}.{models}",
            self.item,
            self.kind.word(),
            self.session,
            minute(self.from_ms),
            minute(self.to_ms),
            total(&self.models).text()
        )
    }

    /// The comment: the line for a person, then the JSON of the report
    /// in a `details` block with the mark [`MARK`].
    pub fn comment(&self) -> String {
        let json = serde_json::to_string(self).unwrap_or_default();
        format!(
            "riff usage: {}\n\n<details><summary>{MARK}</summary>\n\n```json\n{json}\n```\n\n</details>\n",
            self.line()
        )
    }

    /// The report in the body of a comment. `None` for a comment with
    /// no mark, or whose JSON does not read. Its texts are as the
    /// writer of the comment gave them: see [`Report::valid`].
    pub fn parse(body: &str) -> Option<Report> {
        let (_, block) = body.split_once(&format!("<summary>{MARK}</summary>"))?;
        block
            .lines()
            .map(str::trim)
            .find(|line| line.starts_with('{'))
            .and_then(|json| serde_json::from_str(json).ok())
    }

    /// True when `other` is a report of the same claim.
    fn same_claim(&self, other: &Report) -> bool {
        (&self.item, &self.session, self.from_ms) == (&other.item, &other.session, other.from_ms)
    }
}

/// The minute of a time, in UTC.
fn minute(at_ms: u64) -> String {
    i64::try_from(at_ms)
        .ok()
        .and_then(chrono::DateTime::from_timestamp_millis)
        .map_or_else(
            || at_ms.to_string(),
            |at| at.format("%Y-%m-%d %H:%M").to_string(),
        )
}

/// One comment of an issue.
#[derive(Debug, Deserialize)]
pub struct Comment {
    pub id: u64,
    /// Who wrote the comment.
    #[serde(deserialize_with = "crate::text::forge_de")]
    pub login: String,
    pub body: String,
}

/// A report that `riff usage` counts, and who wrote its comment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Counted {
    pub report: Report,
    pub login: String,
}

/// The reports of `issue` in `comments`, each claim of each person one
/// time (01M3Y1YP3QMKS6B35PJ42KNYXX): of two comments of one person for
/// one claim, the later one counts. A comment of another person never
/// replaces a report: it counts as a report of that person. A report
/// that is not valid for the issue counts for nothing
/// ([`Report::valid`]).
pub fn counted(issue: u64, comments: &[Comment]) -> Vec<Counted> {
    let mut out: Vec<Counted> = Vec::new();
    for comment in comments {
        let Some(report) = Report::parse(&comment.body).filter(|r| r.valid(issue)) else {
            continue;
        };
        // GitHub gives the login. It is a name too, with `[bot]` for an app.
        let login: String = comment
            .login
            .chars()
            .take(NAME_CHARS)
            .map(|c| match name_char(c) || matches!(c, '[' | ']') {
                true => c,
                false => '-',
            })
            .collect();
        out.retain(|c| !(c.login == login && c.report.same_claim(&report)));
        out.push(Counted { report, login });
    }
    out
}

/// The tokens of all of `counted`.
pub fn sum(counted: &[Counted]) -> Tokens {
    counted.iter().map(|c| total(&c.report.models)).sum()
}

/// The issues of one repository on GitHub, through the `gh` of the
/// machine.
pub struct Forge<'a> {
    gh: &'a Gh,
    /// `OWNER/REPO`.
    repo: &'a str,
}

impl<'a> Forge<'a> {
    /// The forge of the repository of `thread`. `None` when the thread
    /// is not the thread of a repository.
    pub fn of(gh: &'a Gh, thread: &'a str) -> Option<Forge<'a>> {
        let (owner, name) = thread.split_once('/')?;
        let part = |p: &str| !p.is_empty() && riff_core::name::sanitize(p) == p;
        (part(owner) && part(name)).then_some(Forge { gh, repo: thread })
    }

    /// Each comment of `issue`, oldest first.
    pub fn comments(&self, issue: u64) -> Result<Vec<Comment>> {
        let path = format!("repos/{}/issues/{issue}/comments", self.repo);
        let jq = ".[] | {id, login: .user.login, body}";
        let out = self
            .gh
            .run(&["api", "--paginate", &path, "--jq", jq], None)?;
        serde_json::Deserializer::from_str(&out)
            .into_iter::<Comment>()
            .collect::<Result<_, _>>()
            .with_context(|| format!("gh api {path}"))
    }

    fn add(&self, issue: u64, body: &str) -> Result<()> {
        let path = format!("repos/{}/issues/{issue}/comments", self.repo);
        self.send("POST", &path, body)
    }

    fn edit(&self, comment: u64, body: &str) -> Result<()> {
        let path = format!("repos/{}/issues/comments/{comment}", self.repo);
        self.send("PATCH", &path, body)
    }

    fn send(&self, method: &str, path: &str, body: &str) -> Result<()> {
        let input = serde_json::json!({ "body": body }).to_string();
        self.gh
            .run(&["api", "-X", method, path, "--input", "-"], Some(&input))?;
        Ok(())
    }

    fn login(&self) -> Result<String> {
        let out = self.gh.run(&["api", "user", "--jq", ".login"], None)?;
        Ok(text::forge(out.trim()))
    }

    /// Puts `report` on `issue`: it replaces the comment of the same
    /// claim that this person wrote, else it adds a comment
    /// (01M3Y1YP2TVYQC7GCCAMN6111K). Then it writes the total of the
    /// issue again, when the issue has one. A total that it cannot
    /// write does not fail the report: it returns why
    /// (01M3ZRQY9F9P7DF187Q0PJDS30).
    pub fn publish(&self, issue: u64, report: &Report) -> Result<Option<String>> {
        let comments = self.comments(issue)?;
        let same: Vec<&Comment> = comments
            .iter()
            .filter(|c| Report::parse(&c.body).is_some_and(|r| r.same_claim(report)))
            .collect();
        let mine = match same.is_empty() {
            true => None,
            false => {
                let login = self.login()?;
                same.into_iter().find(|c| c.login == login)
            }
        };
        match mine {
            Some(comment) => self.edit(comment.id, &report.comment())?,
            None => self.add(issue, &report.comment())?,
        }
        // The comment of the claim is on the issue. GitHub can refuse
        // the edit of a total that another person wrote: that is no
        // failure of the report.
        if comments.iter().any(|c| c.body.starts_with(TOTAL_MARK)) {
            return Ok(self.total(issue).err().map(|e| format!("{e:#}")));
        }
        Ok(None)
    }

    /// Writes the comment with the total of `issue`
    /// (01M3Y1YP514MPX8DTKMTWDHE8Q): it replaces the total that the
    /// issue has, else it adds one. It returns the total. An issue with
    /// no report gets no comment.
    pub fn total(&self, issue: u64) -> Result<Tokens> {
        let comments = self.comments(issue)?;
        let counted = counted(issue, &comments);
        if counted.is_empty() {
            return Ok(Tokens::default());
        }
        let body = total_comment(issue, &counted);
        match comments.iter().find(|c| c.body.starts_with(TOTAL_MARK)) {
            Some(old) => {
                let login = self.login()?;
                if old.login != login {
                    anyhow::bail!(
                        "the total comment is of the account {}, and GitHub lets only that account edit it",
                        old.login
                    );
                }
                self.edit(old.id, &body)?
            }
            None => self.add(issue, &body)?,
        }
        Ok(sum(&counted))
    }

    /// The number and the title of each issue of the wave `title`, open
    /// and closed, in the order of the numbers.
    pub fn wave(&self, title: &str) -> Result<Vec<(u64, String)>> {
        #[derive(Deserialize)]
        struct Issue {
            number: u64,
            #[serde(deserialize_with = "crate::text::forge_de")]
            title: String,
        }
        let mut issues: Vec<Issue> = self.gh.json(&[
            "issue",
            "list",
            "--repo",
            self.repo,
            "--milestone",
            title,
            "--state",
            "all",
            "--limit",
            "500",
            "--json",
            "number,title",
        ])?;
        issues.sort_by_key(|i| i.number);
        Ok(issues.into_iter().map(|i| (i.number, i.title)).collect())
    }
}

/// The comment with the total of `issue`: one line for a person, after
/// [`TOTAL_MARK`].
///
/// ```
/// use riff::usage::{Counted, Kind, Report, Tokens, total_comment};
///
/// let counted = |kind, model: &str, output| Counted {
///     report: Report {
///         item: "issue-7".into(),
///         kind,
///         session: "a6cf0123".into(),
///         from_ms: 1,
///         to_ms: 2,
///         models: [(model.to_owned(), Tokens { output, ..Tokens::default() })].into(),
///     },
///     login: "mike".into(),
/// };
/// let all = [counted(Kind::Work, "opus", 1_000), counted(Kind::Verify, "haiku", 200)];
/// assert_eq!(
///     total_comment(7, &all),
///     "<!-- riff:usage-total -->\nriff usage total of #7: 1,200 tokens (input 0, output 1,200, \
///      cache write 0, cache read 0) in 2 claims. Work: 1,000 tokens. Verify: 200 tokens. \
///      Models: haiku, opus.\n"
/// );
/// ```
pub fn total_comment(issue: u64, counted: &[Counted]) -> String {
    let of = |kind| {
        let of_kind: Vec<Counted> = counted
            .iter()
            .filter(|c| c.report.kind == kind)
            .cloned()
            .collect();
        count(sum(&of_kind).total())
    };
    let models: BTreeSet<&str> = counted
        .iter()
        .flat_map(|c| c.report.models.keys().map(String::as_str))
        .collect();
    let models: Vec<&str> = models.into_iter().collect();
    format!(
        "{TOTAL_MARK}\nriff usage total of #{issue}: {} in {}. Work: {} tokens. Verify: {} tokens. Models: {}.\n",
        sum(counted).text(),
        claims(counted.len()),
        of(Kind::Work),
        of(Kind::Verify),
        models.join(", ")
    )
}

fn claims(n: usize) -> String {
    match n {
        1 => "1 claim".into(),
        n => format!("{n} claims"),
    }
}

/// What a report of an ended claim did.
#[derive(Debug)]
pub enum Outcome {
    /// The sum is a comment on the issue. `total` says why the total
    /// comment of the issue is not updated, when riff could not write it.
    Posted { issue: u64, total: Option<String> },
    /// The sum stays in the marks of this machine, and why.
    Kept { why: String },
    /// The claim has no tokens: riff writes no comment.
    Empty,
}

/// Puts the sum of the ended `claim` of `session` on its issue. It
/// never fails (01M3Y1YP39VFX6GH33H7B8A8KR): each cause of no comment is
/// an [`Outcome::Kept`]. A claim with no tokens gets no comment.
pub fn report(gh: &Gh, session: &str, claim: &Claim) -> Outcome {
    if total(&claim.models).total() == 0 {
        return Outcome::Empty;
    }
    let kept = |why: String| Outcome::Kept { why };
    let Some((issue, report)) = Report::of(claim, session) else {
        return kept(format!("{} names no issue", claim.item));
    };
    let Some(forge) = Forge::of(gh, &claim.thread) else {
        return kept(format!("{} is no repository", claim.thread));
    };
    match forge.publish(issue, &report) {
        Ok(total) => Outcome::Posted { issue, total },
        Err(e) => kept(format!("riff cannot write the comment on #{issue}: {e:#}")),
    }
}

/// The line of a release for the ended `claim` and the `outcome` of its
/// report.
///
/// ```
/// use riff::usage::{Claim, Outcome, Tokens, reported};
///
/// let claim = Claim {
///     thread: "acme/app".into(),
///     item: "issue-7".into(),
///     from_ms: 1,
///     to_ms: Some(2),
///     models: [("opus".to_owned(), Tokens { output: 1_500, ..Tokens::default() })].into(),
/// };
/// assert_eq!(
///     reported(&claim, &Outcome::Posted { issue: 7, total: None }),
///     "The claim of issue-7 took 1,500 tokens (input 0, output 1,500, cache write 0, \
///      cache read 0). riff put them on #7 as a comment."
/// );
/// let why = "the total comment is of the account bot, and the forge lets only that account edit it";
/// assert_eq!(
///     reported(&claim, &Outcome::Posted { issue: 7, total: Some(why.into()) }),
///     "The claim of issue-7 took 1,500 tokens (input 0, output 1,500, cache write 0, \
///      cache read 0). riff put them on #7 as a comment. The total comment of #7 is not \
///      updated: the total comment is of the account bot, and the forge lets only that \
///      account edit it."
/// );
/// assert_eq!(
///     reported(&claim, &Outcome::Kept { why: "gh is not installed".into() }),
///     "The claim of issue-7 took 1,500 tokens (input 0, output 1,500, cache write 0, \
///      cache read 0). They stay on this machine: gh is not installed. `riff usage` shows \
///      them here."
/// );
/// let idle = Claim { models: Default::default(), ..claim };
/// assert_eq!(
///     reported(&idle, &Outcome::Empty),
///     "riff counted no tokens for the claim of issue-7."
/// );
/// ```
pub fn reported(claim: &Claim, outcome: &Outcome) -> String {
    let took = format!(
        "The claim of {} took {}.",
        claim.item,
        total(&claim.models).text()
    );
    match outcome {
        Outcome::Posted { issue, total: None } => {
            format!("{took} riff put them on #{issue} as a comment.")
        }
        Outcome::Posted {
            issue,
            total: Some(why),
        } => format!(
            "{took} riff put them on #{issue} as a comment. {}",
            total_not_updated(*issue, why)
        ),
        Outcome::Kept { why } => {
            format!("{took} They stay on this machine: {why}. `riff usage` shows them here.")
        }
        Outcome::Empty => format!("riff counted no tokens for the claim of {}.", claim.item),
    }
}

/// The words for a total comment of `issue` that riff could not write,
/// and `why` (01M3ZRQY9F9P7DF187Q0PJDS30).
///
/// ```
/// assert_eq!(
///     riff::usage::total_not_updated(7, "the total comment is of the account bot"),
///     "The total comment of #7 is not updated: the total comment is of the account bot."
/// );
/// ```
pub fn total_not_updated(issue: u64, why: &str) -> String {
    format!("The total comment of #{issue} is not updated: {why}.")
}

/// The marks of this machine and its `gh`: what counts the claims of a
/// session, and reports them. A test gives a directory and a `gh` of
/// its own.
pub struct Meter {
    /// The directory of the marks ([`crate::local::marks`]).
    pub dir: PathBuf,
    pub gh: Gh,
}

impl Meter {
    /// The meter of this machine. `None` on a machine with no directory
    /// for the marks.
    pub fn here() -> Option<Meter> {
        Some(Meter {
            dir: crate::local::marks()?,
            gh: Gh::default(),
        })
    }

    /// Records the transcript of `session`. A failure goes to stderr.
    pub fn saw(&self, session: &str, transcript: &Path) {
        if let Err(e) = saw(&self.dir, session, transcript) {
            eprintln!("riff: cannot record the transcript of the session: {e}");
        }
    }

    /// Records the claim of `item` in `thread` of `session`, from now.
    /// A failure goes to stderr.
    pub fn started(&self, session: &str, thread: &str, item: &str) {
        if let Err(e) = started(&self.dir, session, thread, item, now_ms()) {
            eprintln!("riff: cannot record the start of the claim of {item}: {e}");
        }
    }

    /// Ends the claim of `item` in `thread` of `session` now, sums it,
    /// and puts the sum on its issue. It returns the line for the
    /// session, and none for a claim with no start time in the marks.
    /// It never fails.
    pub fn release(&self, session: &str, thread: &str, item: &str) -> Option<String> {
        match ended(&self.dir, session, thread, item, now_ms()) {
            Ok(Some(claim)) => Some(reported(&claim, &report(&self.gh, session, &claim))),
            Ok(None) => None,
            Err(e) => Some(format!("riff cannot count the tokens of {item}: {e}.")),
        }
    }

    /// True when `session` has an open claim in the marks.
    pub fn holds(&self, session: &str) -> bool {
        load(&self.dir, session)
            .claims
            .iter()
            .any(|c| c.to_ms.is_none())
    }

    /// Ends each open claim of `session` that started before `now`,
    /// sums each one, and puts each sum on its issue. It returns one
    /// line for each claim. It never fails.
    pub fn release_all(&self, session: &str, now: u64) -> Vec<String> {
        match ended_all(&self.dir, session, now) {
            Ok(done) => done
                .iter()
                .map(|claim| reported(claim, &report(&self.gh, session, claim)))
                .collect(),
            Err(e) => vec![format!("riff cannot count the tokens of the claims: {e}.")],
        }
    }

    /// After the merge of the pull request of `issue`: puts the open
    /// claim of the issue of `session` on the issue as it is now, then
    /// writes the total of the issue (01M3Y1YP514MPX8DTKMTWDHE8Q). The
    /// release of the claim replaces the first comment. It returns the
    /// line for the session.
    pub fn merged(&self, session: Option<&str>, thread: &str, issue: u64) -> Result<String> {
        let forge =
            Forge::of(&self.gh, thread).with_context(|| format!("{thread} is no repository"))?;
        let item = format!("issue-{issue}");
        let open = session.and_then(|id| {
            let claim = so_far(&self.dir, id, &item, now_ms())?;
            Some(Report::of(&claim, id)?.1).filter(|r| total(&r.models).total() > 0)
        });
        let posted = match open {
            Some(report) => {
                forge.publish(issue, &report)?;
                format!("The comment of the claim of #{issue} is on the issue. ")
            }
            None => String::new(),
        };
        let total = match forge.total(issue) {
            Ok(total) => total,
            Err(e) => {
                return Ok(format!(
                    "{posted}{}",
                    total_not_updated(issue, &format!("{e:#}"))
                ));
            }
        };
        Ok(match total.total() {
            0 => format!("#{issue} has no comment with tokens."),
            _ => format!("The total of #{issue} is on the issue: {}.", total.text()),
        })
    }
}

/// The line of `riff usage ISSUE` for the comments of `issue` with the
/// mark [`MARK`] that it did not count, with how many for each cause
/// (01M3ZRQY9F9P7DF187Q0PJDS30). `None` when it counted each one.
///
/// ```
/// use riff::usage::{Comment, uncounted};
///
/// let marked = |id, json: &str| Comment {
///     id,
///     login: "mallory".into(),
///     body: format!("riff usage\n\n<details><summary>riff:usage</summary>\n\n```json\n{json}\n```\n"),
/// };
/// let report = |item: &str, session: &str| format!(
///     r#"{{"item":"{item}","kind":"work","session":"{session}","from_ms":1,"to_ms":2,"models":{{}}}}"#
/// );
/// let comments = [
///     marked(1, &report("issue-8", "a6cf0123")),
///     marked(2, &report("issue-7", "a6cf 0123")),
///     marked(3, "not json"),
///     marked(4, &report("issue-7", "a6cf0123")),
///     Comment { id: 5, login: "mike".into(), body: "LGTM".into() },
/// ];
/// assert_eq!(
///     uncounted(7, &comments).unwrap(),
///     "riff did not count 3 comments with the mark riff:usage: 1 with an item of another \
///      issue, 1 with a text that is no name, 1 with no report that riff can read."
/// );
/// assert_eq!(uncounted(7, &comments[3..]), None);
/// ```
pub fn uncounted(issue: u64, comments: &[Comment]) -> Option<String> {
    let mark = format!("<summary>{MARK}</summary>");
    let (mut other, mut no_name, mut unread) = (0, 0, 0);
    for comment in comments.iter().filter(|c| c.body.contains(&mark)) {
        match Report::parse(&comment.body) {
            None => unread += 1,
            Some(r) if r.valid(issue) => {}
            Some(r)
                if issue_of(&r.item) == Some((issue, r.kind))
                    && r.item == claim_item(issue, r.kind) =>
            {
                no_name += 1
            }
            Some(_) => other += 1,
        }
    }
    let causes: Vec<String> = [
        (other, "with an item of another issue"),
        (no_name, "with a text that is no name"),
        (unread, "with no report that riff can read"),
    ]
    .into_iter()
    .filter(|(n, _)| *n > 0)
    .map(|(n, cause)| format!("{n} {cause}"))
    .collect();
    let all = other + no_name + unread;
    let comments = match all {
        1 => "1 comment".to_owned(),
        n => format!("{n} comments"),
    };
    (all > 0).then(|| {
        format!(
            "riff did not count {comments} with the mark {MARK}: {}.",
            causes.join(", ")
        )
    })
}

/// The text of `riff usage ISSUE`: the total, then the work claims and
/// the verify claims, each with its models and with who wrote its
/// comment (01M3Y1YP3QMKS6B35PJ42KNYXX).
///
/// ```
/// use riff::usage::{Counted, Kind, Report, Tokens, issue_text};
///
/// let counted = Counted {
///     report: Report {
///         item: "issue-7".into(),
///         kind: Kind::Work,
///         session: "a6cf0123".into(),
///         from_ms: 1_790_931_600_000,
///         to_ms: 1_790_935_200_000,
///         models: [("opus".to_owned(), Tokens { output: 1_500, ..Tokens::default() })].into(),
///     },
///     login: "mike".into(),
/// };
/// assert_eq!(
///     issue_text(7, &[counted]),
///     "#7: 1,500 tokens (input 0, output 1,500, cache write 0, cache read 0) in 1 claim\n\
///      work: 1,500 tokens\n\
///      \x20 issue-7, work, session a6cf0123, 2026-10-02 09:00 to 2026-10-02 10:00 UTC: 1,500 \
///      tokens (input 0, output 1,500, cache write 0, cache read 0). Models: opus. Comment of \
///      mike.\n\
///      \x20   opus: 1,500 tokens (input 0, output 1,500, cache write 0, cache read 0)\n"
/// );
/// assert_eq!(issue_text(7, &[]), "#7 has no comment with tokens.\n");
/// ```
pub fn issue_text(issue: u64, counted: &[Counted]) -> String {
    if counted.is_empty() {
        return format!("#{issue} has no comment with tokens.\n");
    }
    let mut out = format!(
        "#{issue}: {} in {}\n",
        sum(counted).text(),
        claims(counted.len())
    );
    for kind in [Kind::Work, Kind::Verify] {
        let of_kind: Vec<Counted> = counted
            .iter()
            .filter(|c| c.report.kind == kind)
            .cloned()
            .collect();
        if of_kind.is_empty() {
            continue;
        }
        let _ = writeln!(
            out,
            "{}: {} tokens",
            kind.word(),
            count(sum(&of_kind).total())
        );
        for c in &of_kind {
            let _ = writeln!(out, "  {} Comment of {}.", c.report.line(), c.login);
            for (model, tokens) in &c.report.models {
                let _ = writeln!(out, "    {model}: {}", tokens.text());
            }
        }
    }
    out
}

/// The text of `riff usage --wave`: each issue of the wave with its
/// total, and the sum (01M3Y1YP45VQS5HMJCXKRN3CCR).
///
/// ```
/// use riff::usage::{Tokens, wave_text};
///
/// let tokens = |output| Tokens { output, ..Tokens::default() };
/// let rows = [(7, "Show the wave".to_owned(), tokens(1_500)), (9, "Fix it".to_owned(), tokens(0))];
/// assert_eq!(
///     wave_text("Wave 3", &rows),
///     "Wave 3: 1,500 tokens (input 0, output 1,500, cache write 0, cache read 0) in 2 issues\n\
///      \x20 #7 Show the wave: 1,500 tokens\n\
///      \x20 #9 Fix it: 0 tokens\n"
/// );
/// ```
pub fn wave_text(title: &str, rows: &[(u64, String, Tokens)]) -> String {
    let sum: Tokens = rows.iter().map(|(_, _, tokens)| *tokens).sum();
    let issues = match rows.len() {
        1 => "1 issue".into(),
        n => format!("{n} issues"),
    };
    let mut out = format!("{title}: {} in {issues}\n", sum.text());
    for (number, name, tokens) in rows {
        let _ = writeln!(out, "  #{number} {name}: {} tokens", count(tokens.total()));
    }
    out
}

/// The text of `riff usage` with no item: each session with marks in
/// `dir`, with the tokens of each item and the tokens for no issue
/// (01M3Y1YP4KVHK1DTZ85YNGDG0T). An open claim counts up to `now`.
pub fn machine_text(dir: &Path, now: u64) -> String {
    let mut out = String::new();
    for session in sessions(dir) {
        let ledger = load(dir, &session);
        let replies = Replies::read(&ledger.transcripts);
        let mut items: BTreeMap<&str, Tokens> = BTreeMap::new();
        for (i, claim) in ledger.claims.iter().enumerate() {
            *items.entry(&claim.item).or_default() += total(&ledger.sum(&replies, i, now));
        }
        let all = total(&replies.sum(&[(0, u64::MAX)]));
        let claimed: Tokens = items.values().copied().sum();
        let _ = writeln!(out, "session {session}: {}", all.text());
        for (item, tokens) in &items {
            let _ = writeln!(out, "  {item}: {}", tokens.text());
        }
        let _ = writeln!(out, "  no issue: {}", all.less(&claimed).text());
    }
    if out.is_empty() {
        out.push_str("No session of this machine has tokens that riff counted.\n");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOUR: u64 = 3_600_000;
    /// 2026-10-02 09:00 UTC.
    const NINE: u64 = 1_790_931_600_000;

    fn reply(at_ms: u64, id: &str, model: &str, output: u64) -> String {
        let at = chrono::DateTime::from_timestamp_millis(at_ms as i64)
            .unwrap()
            .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        format!(
            r#"{{"type":"assistant","timestamp":"{at}","message":{{"id":"{id}","model":"{model}","usage":{{"input_tokens":1,"output_tokens":{output},"cache_creation_input_tokens":0,"cache_read_input_tokens":0}}}}}}"#
        )
    }

    fn write(path: &Path, lines: &[String]) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, lines.join("\n")).unwrap();
    }

    #[test]
    fn a_reply_on_three_lines_counts_one_time_with_its_last_line() {
        let mut replies = Replies::default();
        replies.add(
            &[
                reply(NINE, "m1", "opus", 1),
                reply(NINE + 1, "m1", "opus", 4),
                reply(NINE + 2, "m1", "opus", 9),
            ]
            .join("\n"),
        );
        let models = replies.sum(&[(0, u64::MAX)]);
        assert_eq!(models["opus"].output, 9);
        assert_eq!(models["opus"].input, 1);
    }

    #[test]
    fn a_line_that_is_no_reply_is_skipped() {
        let mut replies = Replies::default();
        replies.add(
            &[
                r#"{"type":"user","timestamp":"2026-10-02T09:00:00Z","message":{"content":"hi"}}"#,
                r#"{"type":"mode","mode":"auto"}"#,
                "no JSON",
                r#"{"type":"assistant","timestamp":"soon","message":{"id":"m","model":"opus","usage":{"output_tokens":5}}}"#,
                r#"{"type":"assistant","timestamp":"2026-10-02T09:00:00Z","message":{"id":"s","model":"<synthetic>","usage":{"output_tokens":0}}}"#,
            ]
            .join("\n"),
        );
        assert!(replies.sum(&[(0, u64::MAX)]).is_empty());
    }

    #[test]
    fn only_a_reply_from_the_start_to_before_the_end_counts() {
        let mut replies = Replies::default();
        replies.add(
            &[
                reply(NINE - 1, "before", "opus", 1),
                reply(NINE, "first", "opus", 10),
                reply(NINE + HOUR - 1, "last", "opus", 100),
                reply(NINE + HOUR, "after", "opus", 1_000),
            ]
            .join("\n"),
        );
        assert_eq!(replies.sum(&[(NINE, NINE + HOUR)])["opus"].output, 110);
    }

    #[test]
    fn the_helper_agents_of_a_transcript_count() {
        let dir = tempfile::tempdir().unwrap();
        let main = dir.path().join("s1.jsonl");
        write(&main, &[reply(NINE, "m1", "opus", 1)]);
        write(
            &dir.path().join("s1/subagents/agent-a.jsonl"),
            &[reply(NINE, "h1", "haiku", 2)],
        );
        write(
            &dir.path().join("s1/subagents/notes.txt"),
            &[reply(NINE, "x", "opus", 50)],
        );
        let gone = dir.path().join("gone.jsonl");
        let models = Replies::read(&[main, gone]).sum(&[(0, u64::MAX)]);
        assert_eq!((models["opus"].output, models["haiku"].output), (1, 2));
    }

    #[test]
    fn a_claim_sums_each_transcript_of_its_session() {
        let marks = tempfile::tempdir().unwrap();
        let (one, two) = (
            marks.path().join("one.jsonl"),
            marks.path().join("two.jsonl"),
        );
        write(&one, &[reply(NINE + 1, "m1", "opus", 1)]);
        write(&two, &[reply(NINE + 2, "m2", "haiku", 2)]);
        saw(marks.path(), "a6cf", &one).unwrap();
        started(marks.path(), "a6cf", "acme/app", "issue-7", NINE).unwrap();
        // `/clear` starts a new transcript.
        saw(marks.path(), "a6cf", &two).unwrap();
        let claim = ended(marks.path(), "a6cf", "acme/app", "issue-7", NINE + HOUR)
            .unwrap()
            .unwrap();
        assert_eq!(total(&claim.models).output, 3);
        assert_eq!(claim.models.len(), 2);
        // The session of another ID has no marks.
        assert_eq!(load(marks.path(), "b2"), Ledger::default());
    }

    #[test]
    fn a_second_claim_of_a_held_item_keeps_the_first_start() {
        let marks = tempfile::tempdir().unwrap();
        started(marks.path(), "a6cf", "acme/app", "issue-7", 10).unwrap();
        started(marks.path(), "a6cf", "acme/app", "issue-7", 20).unwrap();
        let claims = load(marks.path(), "a6cf").claims;
        assert_eq!(claims.len(), 1);
        assert_eq!(claims[0].from_ms, 10);
    }

    #[test]
    fn tokens_in_a_time_with_two_claims_count_for_the_claim_that_started_last() {
        let marks = tempfile::tempdir().unwrap();
        let transcript = marks.path().join("t.jsonl");
        write(
            &transcript,
            &[
                reply(NINE + 1, "work1", "opus", 1),
                reply(NINE + HOUR + 1, "verify", "opus", 10),
                reply(NINE + 2 * HOUR + 1, "work2", "opus", 100),
            ],
        );
        let dir = marks.path();
        saw(dir, "a6cf", &transcript).unwrap();
        started(dir, "a6cf", "acme/app", "issue-7", NINE).unwrap();
        started(dir, "a6cf", "acme/app", "verify-issue-9", NINE + HOUR).unwrap();
        let verify = ended(dir, "a6cf", "acme/app", "verify-issue-9", NINE + 2 * HOUR)
            .unwrap()
            .unwrap();
        let work = ended(dir, "a6cf", "acme/app", "issue-7", NINE + 3 * HOUR)
            .unwrap()
            .unwrap();
        assert_eq!(total(&verify.models).output, 10);
        assert_eq!(total(&work.models).output, 101);
    }

    #[test]
    fn a_new_start_ends_each_open_claim_that_started_before_it() {
        let marks = tempfile::tempdir().unwrap();
        let dir = marks.path();
        assert!(ended_all(dir, "a6cf", 50).unwrap().is_empty());
        started(dir, "a6cf", "acme/app", "issue-7", 10).unwrap();
        started(dir, "a6cf", "acme/app", "issue-8", 20).unwrap();
        // A claim of the new context, from after the start.
        started(dir, "a6cf", "acme/app", "issue-9", 60).unwrap();
        let done = ended_all(dir, "a6cf", 50).unwrap();
        let items: Vec<&str> = done.iter().map(|c| c.item.as_str()).collect();
        assert_eq!(items, ["issue-7", "issue-8"]);
        assert!(done.iter().all(|c| c.to_ms == Some(50)));
        assert_eq!(load(dir, "a6cf").claims[2].to_ms, None);
        assert!(ended_all(dir, "a6cf", 50).unwrap().is_empty());
    }

    #[test]
    fn the_tokens_outside_each_claim_are_for_no_issue() {
        let marks = tempfile::tempdir().unwrap();
        let transcript = marks.path().join("t.jsonl");
        write(
            &transcript,
            &[
                reply(NINE - 1, "idle", "opus", 5),
                reply(NINE + 1, "work", "opus", 70),
            ],
        );
        let dir = marks.path();
        saw(dir, "a6cf", &transcript).unwrap();
        started(dir, "a6cf", "acme/app", "issue-7", NINE).unwrap();
        ended(dir, "a6cf", "acme/app", "issue-7", NINE + HOUR).unwrap();
        let no_issue = load(dir, "a6cf").no_issue(NINE + 2 * HOUR);
        assert_eq!((no_issue.output, no_issue.input), (5, 1));
        let text = machine_text(dir, NINE + 2 * HOUR);
        assert_eq!(
            text,
            "session a6cf: 77 tokens (input 2, output 75, cache write 0, cache read 0)\n\
             \x20 issue-7: 71 tokens (input 1, output 70, cache write 0, cache read 0)\n\
             \x20 no issue: 6 tokens (input 1, output 5, cache write 0, cache read 0)\n"
        );
    }

    #[test]
    fn an_open_claim_counts_so_far_and_the_marks_do_not_change() {
        let marks = tempfile::tempdir().unwrap();
        let transcript = marks.path().join("t.jsonl");
        write(&transcript, &[reply(NINE + 1, "work", "opus", 70)]);
        let dir = marks.path();
        saw(dir, "a6cf", &transcript).unwrap();
        started(dir, "a6cf", "acme/app", "issue-7", NINE).unwrap();
        let before = load(dir, "a6cf");
        let claim = so_far(dir, "a6cf", "issue-7", NINE + HOUR).unwrap();
        assert_eq!(claim.to_ms, Some(NINE + HOUR));
        assert_eq!(total(&claim.models).output, 70);
        assert_eq!(load(dir, "a6cf"), before);
        assert_eq!(so_far(dir, "a6cf", "issue-8", NINE + HOUR), None);
    }

    fn comment(id: u64, login: &str, report: &Report) -> Comment {
        Comment {
            id,
            login: login.into(),
            body: report.comment(),
        }
    }

    fn a_report(session: &str, from_ms: u64, output: u64) -> Report {
        Report {
            item: "issue-7".into(),
            kind: Kind::Work,
            session: session.into(),
            from_ms,
            to_ms: from_ms + 1,
            models: [(
                "opus".to_owned(),
                Tokens {
                    output,
                    ..Tokens::default()
                },
            )]
            .into(),
        }
    }

    #[test]
    fn two_comments_of_one_claim_count_one_time_and_the_later_one_counts() {
        let comments = [
            comment(1, "mike", &a_report("a6cf0123", 10, 5)),
            Comment {
                id: 2,
                login: "ann".into(),
                body: "Thank you.".into(),
            },
            comment(3, "brett", &a_report("b2000000", 10, 7)),
            comment(4, "mike", &a_report("a6cf0123", 10, 50)),
        ];
        let counted = counted(7, &comments);
        assert_eq!(sum(&counted).output, 57);
        let logins: Vec<&str> = counted.iter().map(|c| c.login.as_str()).collect();
        assert_eq!(logins, ["brett", "mike"]);
    }

    #[test]
    fn a_comment_of_another_person_for_my_claim_does_not_replace_my_numbers() {
        let comments = [
            comment(1, "mike", &a_report("a6cf0123", 10, 5)),
            comment(2, "mallory", &a_report("a6cf0123", 10, 0)),
        ];
        let counted = counted(7, &comments);
        let rows: Vec<(&str, u64)> = counted
            .iter()
            .map(|c| (c.login.as_str(), total(&c.report.models).output))
            .collect();
        assert_eq!(rows, [("mike", 5), ("mallory", 0)]);
    }

    /// The comment of the verify of PR #440: a forged line, control
    /// characters, a mention and a link (01M3Y9TD41FZBDQBK42FVG89B8).
    #[test]
    fn a_report_with_a_text_that_is_no_name_counts_for_nothing() {
        let forged = |change: fn(&mut Report)| {
            let mut report = a_report("bbbb", 20, 5);
            change(&mut report);
            comment(9, "mallory", &report)
        };
        let comments = [
            comment(1, "mike", &a_report("a6cf0123", 10, 1_000)),
            forged(|r| {
                r.item = "issue-7, work, session ffffffff, 2026-10-02 09:00 to 2026-10-02 10:00 \
                          UTC: 5 tokens. Models: x. Comment of mike.\n  issue-7"
                    .into()
            }),
            forged(|r| r.session = "bbbb\u{1b}[2K\rzz".into()),
            forged(|r| {
                let tokens = r.models.pop_first().unwrap().1;
                let model = "m\u{1b}]0;TITLE\u{7}odel @everyone [link](http://example.test)";
                r.models.insert(model.into(), tokens);
            }),
            // A report of another issue, and a kind that is not the kind of the item.
            forged(|r| r.item = "issue-8".into()),
            forged(|r| r.kind = Kind::Verify),
            forged(|r| r.item = "issue-07".into()),
        ];
        let counted = counted(7, &comments);
        assert_eq!(counted.len(), 1, "{counted:?}");
        let text = issue_text(7, &counted);
        assert_eq!(text.lines().count(), 4, "{text}");
        assert_eq!(text.matches("Comment of").count(), 1, "{text}");
        assert!(
            text.chars().all(|c| c == '\n' || !c.is_control()),
            "{text:?}"
        );
        let total = total_comment(7, &counted);
        for mark in ["@", "http", "\u{1b}", "["] {
            assert!(
                !total.replace(TOTAL_MARK, "").contains(mark),
                "{mark}: {total}"
            );
        }
        // A login with a character that GitHub does not give.
        let odd = [comment(1, "mi\u{1b}ke\nx", &a_report("a6cf0123", 10, 1))];
        assert_eq!(super::counted(7, &odd)[0].login, "mi-ke-x");
    }

    #[test]
    fn the_largest_number_in_a_comment_does_not_stop_the_sum() {
        let mut large = a_report("bbbb", 20, u64::MAX);
        large.models.get_mut("opus").unwrap().input = u64::MAX;
        let comments = [
            comment(1, "mike", &a_report("a6cf0123", 10, 1_000)),
            comment(2, "mallory", &large),
        ];
        let counted = counted(7, &comments);
        assert_eq!(sum(&counted).output, u64::MAX);
        assert_eq!(sum(&counted).total(), u64::MAX);
        assert!(issue_text(7, &counted).starts_with("#7: 18,446,744,073,709,551,615 tokens"));
        assert!(total_comment(7, &counted).contains("18,446,744,073,709,551,615 tokens"));
    }

    #[test]
    fn my_report_has_names_also_for_a_model_with_other_characters() {
        let claim = Claim {
            thread: "acme/app".into(),
            item: "issue-7".into(),
            from_ms: 1,
            to_ms: Some(2),
            models: [
                (
                    "<synthetic model>".to_owned(),
                    Tokens {
                        output: 1,
                        ..Tokens::default()
                    },
                ),
                (
                    "-synthetic-model-".to_owned(),
                    Tokens {
                        output: 2,
                        ..Tokens::default()
                    },
                ),
            ]
            .into(),
        };
        let (issue, report) = Report::of(&claim, "a~b/c d").unwrap();
        assert_eq!(report.session, "a-b-c-d");
        assert_eq!(report.models["-synthetic-model-"].output, 3);
        assert!(report.valid(issue));
        assert_eq!(Report::parse(&report.comment()), Some(report));
    }

    #[test]
    fn a_comment_holds_only_numbers_and_names() {
        let claim = Claim {
            thread: "acme/app".into(),
            item: "issue-7".into(),
            from_ms: NINE,
            to_ms: Some(NINE + HOUR),
            models: Models::new(),
        };
        let (_, report) = Report::of(&claim, "a6cf0123-4567-89ab-cdef-0123456789ab").unwrap();
        let comment = report.comment();
        assert!(
            !comment.contains("4567"),
            "only the start of the session ID"
        );
        assert!(!comment.contains("acme"), "no thread");
        let json = comment.lines().find(|l| l.starts_with('{')).unwrap();
        let value: serde_json::Value = serde_json::from_str(json).unwrap();
        let keys: Vec<&str> = value
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(
            keys,
            ["item", "kind", "session", "from_ms", "to_ms", "models"]
        );
        // An open claim has no report.
        let open = Claim {
            to_ms: None,
            ..claim
        };
        assert_eq!(Report::of(&open, "a6cf"), None);
    }

    #[test]
    fn a_thread_that_is_no_repository_has_no_forge() {
        let gh = Gh::default();
        assert!(Forge::of(&gh, "acme/app").is_some());
        for thread in ["chat", "dm:a|b", "acme/", "/app", "a/b/c", "acme/my app"] {
            assert!(Forge::of(&gh, thread).is_none(), "{thread}");
        }
    }

    #[test]
    fn a_report_with_no_issue_or_no_gh_stays_on_the_machine() {
        let claim = |thread: &str, item: &str| Claim {
            thread: thread.into(),
            item: item.into(),
            from_ms: 1,
            to_ms: Some(2),
            models: a_report("a6cf", 1, 5).models,
        };
        let gh = Gh::at("/no/such/gh");
        let why = |claim: &Claim| match report(&gh, "a6cf", claim) {
            Outcome::Kept { why } => why,
            other => panic!("no gh: {other:?}"),
        };
        let idle = Claim {
            models: Models::new(),
            ..claim("acme/app", "issue-7")
        };
        assert!(matches!(report(&gh, "a6cf", &idle), Outcome::Empty));
        assert_eq!(why(&claim("acme/app", "docs")), "docs names no issue");
        assert_eq!(why(&claim("chat", "issue-7")), "chat is no repository");
        let no_gh = why(&claim("acme/app", "issue-7"));
        assert!(
            no_gh.starts_with("riff cannot write the comment on #7: cannot run /no/such/gh"),
            "{no_gh}"
        );
    }
}
