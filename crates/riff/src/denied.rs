//! The refusals of auto mode (01M57XY3Q6HEYQG27FE0CEQ7HB): `riff hook
//! denied`, `riff relay denied` and `riff denials`.
//!
//! # Design
//!
//! Claude Code runs the hook `PermissionDenied` when the auto mode
//! checker refuses a tool call. The plugin runs `riff hook denied` for
//! it, in a session on the host. The settings of each container session
//! run `/opt/riff/riff relay denied` ([`crate::run::settings`]): the lead
//! and each worker of the riff daemon.
//!
//! The hook reads its input, makes one [`Denial`] with no secret
//! ([`Denial::of`], 01M57XY3XXCKBXMVPFBY0C2KDM), adds it to the local
//! file ([`Queue`]) and starts the send, detached. It returns at once,
//! and never waits for riff-server. The send takes each kept denial,
//! oldest first, and posts each one as a note in the thread [`THREAD`]
//! (01M57XY3SEG8HCXPZFBVATQ1W2). It keeps each denial that it did not
//! post, for the next send (01M57XY42CNR16M5G024PWKANV). After the posts,
//! the session leaves the thread, so its `read` shows no record
//! (01M57XY405BSXEH3K3Z977RWAW).
//!
//! ```mermaid
//! sequenceDiagram
//!     participant C as Claude Code
//!     participant H as riff hook denied
//!     participant F as the local file
//!     participant S as the send (detached)
//!     participant D as riff daemon
//!     participant R as riff-server
//!     C->>H: PermissionDenied input
//!     H->>F: one denial, with no secret
//!     H-->>C: exit 0 at once
//!     H->>S: start
//!     S->>F: take each denial
//!     alt a session on the host
//!         S->>R: me: lead? claim?
//!         S->>R: post each line, a note in "denials"
//!     else a container session
//!         S->>D: the denials, on the socket of the session
//!         D->>R: post each line, a note in "denials"
//!     end
//!     S->>R: leave "denials"
//!     S->>F: keep each denial that did not go
//! ```
//!
//! On the host, the send asks riff-server for the lead mark and the
//! claim of the session. In a container, the daemon knows them: the
//! lead has a terminal, and a worker has its item.
//!
//! # The record
//!
//! One line, the tags first ([`line()`], 01M57XY3VP35QDP2KK88ERF82M):
//!
//! ```text
//! denied reason=production-deploy tool=Bash head="git push" role=lead user=mike host=pangolin repo=como-technologies/riff session=25456ff6 claim=-: git push origin v3.2.0
//! ```
//!
//! `riff denials` reads the thread, parses each record ([`Record::parse`])
//! and counts them ([`report`], 01M57XY44K2N3PS9F8NQ6SZQP9).

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use riff_core::name::{SessionUri, ThreadName};
use riff_core::wire::Kind;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::run::Scrub;
use riff_client::api::Api;

/// The thread of the records.
pub const THREAD: &str = "denials";

/// The most characters of the text after the tags.
pub const TEXT_MAX: usize = 300;

/// The most characters of the tag `head`.
pub const HEAD_MAX: usize = 60;

/// The number of records at the end of `riff denials`.
pub const LAST: usize = 20;

/// The name of the local file of the denials that wait for a send.
pub const QUEUE: &str = "denials.jsonl";

/// The thread [`THREAD`].
pub fn thread() -> ThreadName {
    THREAD.parse().expect("denials is a thread name")
}

/// The input of the hook `PermissionDenied`. Each other field goes.
#[derive(Debug, Default, Deserialize)]
pub struct Input {
    /// The session ID of Claude Code.
    #[serde(default)]
    pub session_id: Option<String>,
    /// The name of the tool.
    #[serde(default)]
    pub tool_name: String,
    /// The input of the tool.
    #[serde(default)]
    pub tool_input: Value,
    /// Why auto mode refused it.
    #[serde(default)]
    pub reason: String,
    /// Set in a subagent.
    #[serde(default)]
    pub agent_id: Option<String>,
    /// The MCP server of an MCP tool.
    #[serde(default)]
    pub mcp_server: Option<String>,
}

/// One refusal, with no secret: what the local file keeps.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Denial {
    /// The time, in milliseconds since the Unix epoch.
    pub at_ms: u64,
    /// The tag `reason` ([`reason_tag`]).
    pub reason: String,
    /// The tag `tool` ([`tool_tag`]).
    pub tool: String,
    /// The tag `head` ([`head`]).
    pub head: String,
    /// The command or the input, at most [`TEXT_MAX`] characters.
    pub text: String,
    /// True in a worker in a container.
    #[serde(default)]
    pub worker: bool,
    /// True in a subagent.
    #[serde(default)]
    pub subagent: bool,
}

impl Denial {
    /// The denial of the hook input `input`, with each secret of `scrub`
    /// hidden in each tag and in the text.
    ///
    /// ```
    /// use riff::denied::{Denial, Input};
    /// use riff::run::Scrub;
    ///
    /// let input: Input = serde_json::from_str(r#"{
    ///     "tool_name": "Bash",
    ///     "tool_input": {"command": "GH_TOKEN=gho_s3cret git push origin main"},
    ///     "reason": "[Production Deploy] pushes to main"
    /// }"#)?;
    /// let denial = Denial::of(&input, &Scrub::new(["gho_s3cret".to_owned()]), false, 7);
    /// assert_eq!(denial.reason, "production-deploy");
    /// assert_eq!(denial.head, "git push");
    /// assert_eq!(denial.text, "GH_TOKEN=[secret] git push origin main");
    /// # Ok::<(), serde_json::Error>(())
    /// ```
    pub fn of(input: &Input, scrub: &Scrub, worker: bool, at_ms: u64) -> Denial {
        let clean = |text: &str| scrub.clean(text).into_owned();
        let text = match &input.tool_input {
            Value::Object(map) => match map.get("command") {
                Some(Value::String(command)) => command.clone(),
                _ => input.tool_input.to_string(),
            },
            Value::Null => String::new(),
            other => other.to_string(),
        };
        Denial {
            at_ms,
            reason: reason_tag(&input.reason),
            tool: clean(&tool_tag(&input.tool_name, input.mcp_server.as_deref())),
            head: clean(&head(&input.tool_name, &input.tool_input)),
            text: shorten(&one_line(&clean(&text)), TEXT_MAX),
            worker,
            subagent: input.agent_id.as_deref().is_some_and(|a| !a.is_empty()),
        }
    }
}

/// The tag `reason`: the rule in the square brackets as a slug,
/// `checker-unavailable` when the checker model was not there, else
/// `no-verdict`.
///
/// ```
/// use riff::denied::reason_tag;
///
/// assert_eq!(reason_tag("[Production Deploy]"), "production-deploy");
/// assert_eq!(reason_tag("Blocked: [Git Push to Default Branch] x"), "git-push-to-default-branch");
/// assert_eq!(reason_tag("Classifier unavailable"), "checker-unavailable");
/// assert_eq!(
///     reason_tag("Auto mode could not evaluate this action and is blocking it for safety"),
///     "no-verdict"
/// );
/// assert_eq!(reason_tag("[ ]"), "no-verdict");
/// ```
pub fn reason_tag(reason: &str) -> String {
    if reason.trim_start().starts_with("Classifier unavailable") {
        return "checker-unavailable".into();
    }
    let rule = reason
        .split_once('[')
        .and_then(|(_, rest)| rest.split_once(']'))
        .map(|(rule, _)| slug(rule))
        .unwrap_or_default();
    if rule.is_empty() {
        "no-verdict".into()
    } else {
        rule
    }
}

/// `text` in lower case, with one `-` for each run of other characters
/// than letters and digits.
fn slug(text: &str) -> String {
    let mut out = String::new();
    for c in text.chars() {
        if c.is_alphanumeric() {
            out.extend(c.to_lowercase());
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_end_matches('-').to_owned()
}

/// The tag `tool`: the name of the tool, and `@SERVER` for an MCP tool.
///
/// ```
/// use riff::denied::tool_tag;
///
/// assert_eq!(tool_tag("Bash", None), "Bash");
/// assert_eq!(tool_tag("mcp__docs__read", Some("docs")), "mcp__docs__read@docs");
/// assert_eq!(tool_tag("", None), "-");
/// ```
pub fn tool_tag(tool: &str, server: Option<&str>) -> String {
    let tool = tag(tool);
    match server.filter(|s| !s.is_empty()) {
        Some(server) => format!("{tool}@{}", tag(server)),
        None => tool,
    }
}

/// The tag `head`. For Bash: the first two words of the command after
/// `nice -n N`, `env`, the variables and `cd X &&`. For another tool:
/// the path, else the first field of the input.
///
/// ```
/// use riff::denied::head;
/// use serde_json::json;
///
/// let bash = |c: &str| head("Bash", &json!({"command": c}));
/// assert_eq!(bash("git push origin v3.2.0"), "git push");
/// assert_eq!(bash("cd /x && nice -n 19 env A=1 B=2 cargo test -p riff"), "cargo test");
/// assert_eq!(bash("ls"), "ls");
/// assert_eq!(head("Edit", &json!({"file_path": "/a/b.rs", "old_string": "x"})), "/a/b.rs");
/// assert_eq!(head("WebFetch", &json!({"url": "https://x", "prompt": "p"})), "https://x");
/// assert_eq!(head("Other", &json!({})), "");
/// ```
pub fn head(tool: &str, input: &Value) -> String {
    let field = |name: &str| input.get(name).and_then(Value::as_str);
    let head = if tool == "Bash" {
        bash_head(field("command").unwrap_or_default())
    } else if let Some(path) = ["file_path", "path", "notebook_path"]
        .iter()
        .find_map(|name| field(name))
    {
        path.to_owned()
    } else {
        match input.as_object().and_then(|map| map.values().next()) {
            Some(Value::String(text)) => text.clone(),
            Some(other) => other.to_string(),
            None => String::new(),
        }
    };
    shorten(&one_line(&head), HEAD_MAX).replace('"', "'")
}

/// The first two words of the Bash command `command` after `nice -n N`,
/// `env`, the variables and `cd X &&`.
fn bash_head(command: &str) -> String {
    let words: Vec<&str> = command.split_whitespace().collect();
    let mut i = 0;
    while i < words.len() {
        let word = words[i];
        if word == "cd" && words.get(i + 2).is_some_and(|w| *w == "&&" || *w == ";") {
            i += 3;
        } else if word == "nice" {
            i += 1;
            if words.get(i) == Some(&"-n") {
                i += 2;
            } else if words.get(i).is_some_and(|w| w.starts_with('-')) {
                i += 1;
            }
        } else if word == "env"
            || (word.starts_with('-') && i > 0 && words[i - 1] == "env")
            || is_variable(word)
        {
            i += 1;
        } else {
            break;
        }
    }
    words[i.min(words.len())..]
        .iter()
        .take(2)
        .copied()
        .collect::<Vec<_>>()
        .join(" ")
}

/// True for `NAME=value`.
fn is_variable(word: &str) -> bool {
    word.split_once('=').is_some_and(|(name, _)| {
        !name.is_empty()
            && !name.starts_with(|c: char| c.is_ascii_digit())
            && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
    })
}

/// `text` on one line: each control character becomes a space.
fn one_line(text: &str) -> String {
    text.trim()
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect()
}

/// At most `max` characters of `text`.
fn shorten(text: &str, max: usize) -> String {
    match text.char_indices().nth(max) {
        Some((at, _)) => text[..at].to_owned(),
        None => text.to_owned(),
    }
}

/// A value of a tag with no space: each white space becomes `-`, and an
/// empty value is `-`.
fn tag(value: &str) -> String {
    let value: String = value
        .trim()
        .chars()
        .map(|c| {
            if c.is_whitespace() || c.is_control() {
                '-'
            } else {
                c
            }
        })
        .collect();
    if value.is_empty() { "-".into() } else { value }
}

/// The kind of the session that got the refusal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// The lead of the user.
    Lead,
    /// A worker session that has no container.
    Worker,
    /// A worker of the riff daemon, in a container.
    Container,
    /// Each other session.
    Session,
}

impl Role {
    /// The name of the role.
    pub fn name(self) -> &'static str {
        match self {
            Role::Lead => "lead",
            Role::Worker => "worker",
            Role::Container => "container",
            Role::Session => "session",
        }
    }
}

/// The session of a record: what the send knows, and the hook does not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Where {
    /// The kind of the session.
    pub role: Role,
    /// The user.
    pub user: String,
    /// The host.
    pub host: String,
    /// `OWNER/REPO`, or `-`.
    pub repo: String,
    /// The session ID.
    pub session: String,
    /// The claim of the session, if it has one.
    pub claim: Option<String>,
}

impl Where {
    /// The place of the session `me`, with `role` and `claim`.
    ///
    /// ```
    /// use riff::denied::{Role, Where};
    ///
    /// let me = "riff://mike@pangolin/como-technologies/riff?session=25456ff6-aaaa".parse()?;
    /// let at = Where::of(&me, Role::Lead, None);
    /// assert_eq!((at.user.as_str(), at.host.as_str()), ("mike", "pangolin"));
    /// assert_eq!(at.repo, "como-technologies/riff");
    /// assert_eq!(at.session, "25456ff6-aaaa");
    /// # Ok::<(), riff_core::name::NameError>(())
    /// ```
    pub fn of(me: &SessionUri, role: Role, claim: Option<&str>) -> Where {
        Where {
            role,
            user: me.who().user().to_owned(),
            host: me.place().host().to_owned(),
            repo: me.place().repo_text(),
            session: me.who().session().unwrap_or_default().to_owned(),
            claim: claim.map(str::to_owned),
        }
    }
}

/// The record of `denial` in the session `at`: one line, the tags
/// first. The role ends with `+subagent` in a subagent.
///
/// ```
/// use riff::denied::{Denial, Role, Where, line};
///
/// let denial = Denial {
///     at_ms: 0,
///     reason: "production-deploy".into(),
///     tool: "Bash".into(),
///     head: "git push".into(),
///     text: "git push origin v3.2.0".into(),
///     worker: false,
///     subagent: false,
/// };
/// let me = "riff://mike@pangolin/como-technologies/riff?session=25456ff6-aaaa".parse()?;
/// assert_eq!(
///     line(&denial, &Where::of(&me, Role::Lead, None)),
///     "denied reason=production-deploy tool=Bash head=\"git push\" role=lead user=mike \
///      host=pangolin repo=como-technologies/riff session=25456ff6 claim=-: git push origin v3.2.0"
/// );
/// let sub = Denial { subagent: true, ..denial };
/// let at = Where::of(&me, Role::Container, Some("issue-12"));
/// assert!(line(&sub, &at).contains(" role=container+subagent "));
/// assert!(line(&sub, &at).contains(" claim=issue-12: "));
/// # Ok::<(), riff_core::name::NameError>(())
/// ```
pub fn line(denial: &Denial, at: &Where) -> String {
    let mut role = at.role.name().to_owned();
    if denial.subagent {
        role.push_str("+subagent");
    }
    format!(
        "denied reason={} tool={} head=\"{}\" role={role} user={} host={} repo={} session={} claim={}: {}",
        tag(&denial.reason),
        tag(&denial.tool),
        denial.head.replace('"', "'"),
        tag(&at.user),
        tag(&at.host),
        tag(&at.repo),
        tag(crate::run::short(&at.session)),
        tag(at.claim.as_deref().unwrap_or_default()),
        denial.text,
    )
}

/// The local file of the denials that wait for a send
/// (01M57XY42CNR16M5G024PWKANV): one JSON line for each.
#[derive(Debug, Clone)]
pub struct Queue {
    path: PathBuf,
}

impl Queue {
    /// The file [`QUEUE`] in the folder `dir`.
    pub fn in_dir(dir: &Path) -> Queue {
        Queue {
            path: dir.join(QUEUE),
        }
    }

    /// The file of this process. In a container session: in its Claude
    /// folder, which stays on the host. Else in the local folder of riff
    /// ([`crate::local::dir`]).
    pub fn here() -> Option<Queue> {
        if std::env::var_os(crate::daemon::SOCKET_VAR).is_some_and(|s| !s.is_empty()) {
            let dir = std::env::var_os("CLAUDE_CONFIG_DIR")
                .filter(|d| !d.is_empty())
                .map(PathBuf::from)
                .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".claude")))?;
            return Some(Queue::in_dir(&dir));
        }
        crate::local::dir().map(|dir| Queue::in_dir(&dir))
    }

    /// Adds `denials` at the end of the file, in order.
    ///
    /// ```
    /// use riff::denied::{Denial, Queue};
    ///
    /// let dir = tempfile::tempdir()?;
    /// let queue = Queue::in_dir(dir.path());
    /// let denial = |at_ms| Denial {
    ///     at_ms, reason: "r".into(), tool: "Bash".into(), head: "ls".into(),
    ///     text: "ls".into(), worker: false, subagent: false,
    /// };
    /// queue.keep(&[denial(1)])?;
    /// queue.keep(&[denial(2)])?;
    /// let taken = queue.take();
    /// assert_eq!(taken.iter().map(|d| d.at_ms).collect::<Vec<_>>(), [1, 2]);
    /// assert!(queue.take().is_empty(), "a take empties the file");
    /// # Ok::<(), std::io::Error>(())
    /// ```
    pub fn keep(&self, denials: &[Denial]) -> std::io::Result<()> {
        if denials.is_empty() {
            return Ok(());
        }
        if let Some(dir) = self.path.parent() {
            riff_client::files::make_dir(dir)?;
        }
        let mut text = String::new();
        for denial in denials {
            text.push_str(&serde_json::to_string(denial)?);
            text.push('\n');
        }
        let mut file = riff_client::files::options()
            .create(true)
            .append(true)
            .open(&self.path)?;
        riff_client::files::private(&file)?;
        file.write_all(text.as_bytes())
    }

    /// Waits until no other send holds the lock of the file. The lock
    /// ends when the guard drops, or when the process ends.
    fn wait_turn(&self) -> std::io::Result<std::fs::File> {
        if let Some(dir) = self.path.parent() {
            riff_client::files::make_dir(dir)?;
        }
        let file = riff_client::files::options()
            .create(true)
            .append(true)
            .open(self.path.with_extension("lock"))?;
        file.lock()?;
        Ok(file)
    }

    /// Takes each denial of the file, oldest first, and empties the file.
    /// It moves the file first, so two sends never take the same denial.
    /// A line that is not a denial goes.
    pub fn take(&self) -> Vec<Denial> {
        let taking = self
            .path
            .with_extension(format!("{}.taking", std::process::id()));
        if std::fs::rename(&self.path, &taking).is_err() {
            return Vec::new();
        }
        let text = std::fs::read_to_string(&taking).unwrap_or_default();
        let _ = std::fs::remove_file(&taking);
        text.lines()
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect()
    }
}

/// Posts each line of `lines` as a note in the thread [`THREAD`], as the
/// session `me`, in order. After the first post, the session leaves
/// the thread (01M57XY405BSXEH3K3Z977RWAW). It gives the number of lines
/// that it posted: it stops at the first post that fails.
pub async fn post(api: &Api, me: &SessionUri, lines: &[String]) -> usize {
    let thread = thread();
    let mut posted = 0;
    for line in lines {
        if api
            .post(me, Some(&thread), &[], line, Kind::Note)
            .await
            .is_err()
        {
            break;
        }
        posted += 1;
    }
    if posted > 0 {
        let _ = api.leave(me, &thread).await;
    }
    posted
}

/// The send of a session on the host: it takes each denial of the
/// local file and posts it as the session `me` on `api`. It asks
/// riff-server for the lead mark and the claim of the session first.
/// Each denial that did not go goes back to the file. Two sends of one
/// machine run one after the other: a send that fails puts its
/// denials back before the next send takes the file.
pub async fn send_here(api: &Api, me: &SessionUri, queue: &Queue) -> Result<usize> {
    let _turn = queue.wait_turn()?;
    let denials = queue.take();
    if denials.is_empty() {
        return Ok(0);
    }
    let session = match api.me(me).await {
        Ok(reply) => reply.session,
        Err(e) => {
            queue.keep(&denials)?;
            return Err(e);
        }
    };
    let uri = session.as_ref().map_or(me, |s| &s.uri);
    let claim = uri.claims().first().map(String::as_str);
    let lines: Vec<String> = denials
        .iter()
        .map(|d| {
            let role = if d.worker {
                Role::Worker
            } else if uri.lead() {
                Role::Lead
            } else {
                Role::Session
            };
            line(d, &Where::of(me, role, claim))
        })
        .collect();
    let posted = post(api, me, &lines).await;
    queue.keep(&denials[posted..])?;
    if posted < denials.len() {
        bail!(
            "riff-server took {posted} of {} records: riff keeps the rest",
            denials.len()
        );
    }
    Ok(posted)
}

/// Reads the hook input on `stdin`, and keeps its denial in `queue`
/// with no secret: the values of the secret variables, of the MCP
/// servers of the session and of the sign-in file of `claude`
/// ([`crate::session_relay::secrets_here`]). It gives the session ID of
/// the input.
pub fn keep_input(stdin: &str, queue: &Queue, worker: bool) -> Result<Option<String>> {
    let input: Input = serde_json::from_str(stdin).context("the hook input is no JSON")?;
    let scrub = Scrub::new(crate::session_relay::secrets_here());
    let denial = Denial::of(&input, &scrub, worker, crate::usage::now_ms());
    queue.keep(&[denial])?;
    Ok(input.session_id)
}

/// One record of the thread [`THREAD`]: its tags and its text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Record {
    /// The time of the post, in milliseconds since the Unix epoch.
    pub at_ms: u64,
    /// Each tag, by name.
    pub tags: BTreeMap<String, String>,
    /// The text after the tags.
    pub text: String,
    /// The whole body of the post.
    #[serde(skip)]
    pub body: String,
}

impl Record {
    /// The record of the body `body` of a post at `at_ms`. `None` when
    /// the body is no record.
    ///
    /// ```
    /// use riff::denied::Record;
    ///
    /// let body = "denied reason=x tool=Bash head=\"git push\" role=lead claim=-: git push origin";
    /// let record = Record::parse(body, 5).unwrap();
    /// assert_eq!(record.tag("head"), "git push");
    /// assert_eq!(record.tag("claim"), "-");
    /// assert_eq!(record.tag("none"), "-");
    /// assert_eq!(record.text, "git push origin");
    /// assert!(Record::parse("hello", 5).is_none());
    /// ```
    pub fn parse(body: &str, at_ms: u64) -> Option<Record> {
        let mut rest = body.strip_prefix("denied ")?;
        let mut tags = BTreeMap::new();
        loop {
            let (key, after) = rest.split_once('=')?;
            if key.is_empty() || key.contains(char::is_whitespace) {
                return None;
            }
            let (value, after) = if let Some(quoted) = after.strip_prefix('"') {
                let (value, after) = quoted.split_once('"')?;
                (value, after)
            } else {
                let end = after.find(' ').unwrap_or(after.len());
                let value = &after[..end];
                match value.strip_suffix(':') {
                    Some(value) => (value, &after[value.len()..]),
                    None => (value, &after[end..]),
                }
            };
            tags.insert(key.to_owned(), value.to_owned());
            if let Some(text) = after.strip_prefix(':') {
                return Some(Record {
                    at_ms,
                    tags,
                    body: body.to_owned(),
                    text: text.strip_prefix(' ').unwrap_or(text).to_owned(),
                });
            }
            rest = after.strip_prefix(' ')?;
        }
    }

    /// The value of the tag `name`, or `-`.
    pub fn tag(&self, name: &str) -> &str {
        self.tags.get(name).map_or("-", String::as_str)
    }
}

/// The time of `--since`: a number and a unit, `s`, `m`, `h`, `d` or `w`.
///
/// ```
/// use std::time::Duration;
/// use riff::denied::since;
///
/// assert_eq!(since("7d")?, Duration::from_secs(7 * 86_400));
/// assert_eq!(since("12h")?, Duration::from_secs(12 * 3_600));
/// assert_eq!(since("30m")?, Duration::from_secs(1_800));
/// assert!(since("7").is_err());
/// assert!(since("xd").is_err());
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn since(text: &str) -> Result<Duration> {
    let text = text.trim();
    let wrong =
        || format!("{text:?} is no time: give a number and s, m, h, d or w, for example 7d");
    let unit = text.chars().last().with_context(wrong)?;
    let secs: u64 = match unit {
        's' => 1,
        'm' => 60,
        'h' => 3_600,
        'd' => 86_400,
        'w' => 7 * 86_400,
        _ => bail!(wrong()),
    };
    let number: u64 = text[..text.len() - 1].parse().ok().with_context(wrong)?;
    Ok(Duration::from_secs(number.saturating_mul(secs)))
}

/// The text of `riff denials` for `records`, oldest first: the counts by
/// `reason`, by `tool` and `head`, by `role` and by `repo`, and then the
/// last [`LAST`] records. `span` names the time of the records.
///
/// ```
/// use riff::denied::{Record, report};
///
/// let records: Vec<Record> = [
///     "denied reason=a tool=Bash head=\"git push\" role=lead repo=o/r: git push",
///     "denied reason=a tool=Bash head=\"git push\" role=worker repo=o/r: git push -f",
///     "denied reason=b tool=Edit head=\"/x\" role=worker repo=o/s: {}",
/// ]
/// .iter()
/// .filter_map(|b| Record::parse(b, 0))
/// .collect();
/// let text = report(&records, "in the thread denials");
/// assert!(text.starts_with("3 refusals of auto mode in the thread denials\n"));
/// assert!(text.contains("By reason:\n  2  a\n  1  b\n"));
/// assert!(text.contains("By tool and head:\n  2  Bash git push\n  1  Edit /x\n"));
/// assert!(text.contains("By role:\n  2  worker\n  1  lead\n"));
/// assert!(text.contains("By repo:\n  2  o/r\n  1  o/s\n"));
/// assert!(text.contains("Last 3:\n"));
/// assert_eq!(report(&[], "in the last 7d"), "No refusal of auto mode in the last 7d.\n");
/// ```
pub fn report(records: &[Record], span: &str) -> String {
    if records.is_empty() {
        return format!("No refusal of auto mode {span}.\n");
    }
    let mut out = format!("{} refusals of auto mode {span}\n", records.len());
    let by = |key: &dyn Fn(&Record) -> String| {
        let mut counts: BTreeMap<String, usize> = BTreeMap::new();
        for record in records {
            *counts.entry(key(record)).or_default() += 1;
        }
        let mut counts: Vec<(String, usize)> = counts.into_iter().collect();
        counts.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        counts
    };
    type Key<'k> = &'k dyn Fn(&Record) -> String;
    let groups: [(&str, Key); 4] = [
        ("reason", &|r| r.tag("reason").to_owned()),
        ("tool and head", &|r| {
            format!("{} {}", r.tag("tool"), r.tag("head"))
                .trim_end()
                .to_owned()
        }),
        ("role", &|r| r.tag("role").to_owned()),
        ("repo", &|r| r.tag("repo").to_owned()),
    ];
    for (name, key) in groups {
        out.push_str(&format!("\nBy {name}:\n"));
        for (value, count) in by(key) {
            out.push_str(&format!("  {count}  {value}\n"));
        }
    }
    let last = &records[records.len().saturating_sub(LAST)..];
    out.push_str(&format!("\nLast {}:\n", last.len()));
    for record in last {
        out.push_str(&format!(
            "  {}  {} {} {} {}: {}\n",
            time(record.at_ms),
            record.tag("reason"),
            record.tag("role"),
            record.tag("tool"),
            record.tag("head"),
            record.text
        ));
    }
    out
}

/// The time `at_ms` on the clock of this machine.
fn time(at_ms: u64) -> String {
    use chrono::TimeZone;
    chrono::Local
        .timestamp_millis_opt(at_ms as i64)
        .single()
        .map_or_else(|| "-".into(), |t| t.format("%Y-%m-%d %H:%M").to_string())
}

/// The records of the thread [`THREAD`] at or after `from_ms`, oldest
/// first, as `me` reads them. A thread that the server does not know has
/// none.
pub async fn read(api: &Api, me: &SessionUri, from_ms: u64) -> Result<Vec<Record>> {
    let messages = match api.read(me, &thread(), true).await {
        Ok(messages) => messages,
        Err(e)
            if e.downcast_ref::<riff_client::api::Refusal>()
                .is_some_and(|r| r.status == reqwest::StatusCode::NOT_FOUND) =>
        {
            Vec::new()
        }
        Err(e) => return Err(e),
    };
    Ok(records(
        messages
            .iter()
            .map(|m| (m.message.at_ms, m.message.body.as_str())),
        from_ms,
    ))
}

/// The records of `bodies` (the time and the body of each post, oldest
/// first) at or after `from_ms`.
///
/// ```
/// let bodies = [(1, "denied reason=a: x"), (2, "hello"), (3, "denied reason=b: y")];
/// let records = riff::denied::records(bodies, 2);
/// assert_eq!(records.len(), 1);
/// assert_eq!(records[0].tag("reason"), "b");
/// ```
pub fn records<'a>(bodies: impl IntoIterator<Item = (u64, &'a str)>, from_ms: u64) -> Vec<Record> {
    bodies
        .into_iter()
        .filter(|(at_ms, _)| *at_ms >= from_ms)
        .filter_map(|(at_ms, body)| Record::parse(body, at_ms))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn input(value: Value) -> Input {
        serde_json::from_value(value).unwrap()
    }

    /// 01M57XY3VP35QDP2KK88ERF82M.
    #[test]
    fn the_reason_is_the_rule_as_a_slug() {
        assert_eq!(reason_tag("[Production Deploy]"), "production-deploy");
        assert_eq!(reason_tag("[Self-Modification!!]"), "self-modification");
        assert_eq!(reason_tag("  [  Créer  Fichier ] x"), "créer-fichier");
        assert_eq!(reason_tag("Classifier unavailable"), "checker-unavailable");
        assert_eq!(
            reason_tag("Auto mode could not evaluate this action and is blocking it for safety"),
            "no-verdict"
        );
        assert_eq!(reason_tag(""), "no-verdict");
        assert_eq!(reason_tag("[unclosed"), "no-verdict");
    }

    /// 01M57XY3VP35QDP2KK88ERF82M.
    #[test]
    fn the_head_of_bash_skips_the_prefixes() {
        let bash = |c: &str| head("Bash", &json!({ "command": c }));
        assert_eq!(bash("git push origin v3.2.0"), "git push");
        assert_eq!(bash("nice -n 10 cargo build"), "cargo build");
        assert_eq!(bash("nice -19 cargo build"), "cargo build");
        assert_eq!(bash("env -i FOO=bar just check"), "just check");
        assert_eq!(bash("RUST_LOG=debug cargo run -q"), "cargo run");
        assert_eq!(bash("cd /tmp/x && gh pr merge 12"), "gh pr");
        assert_eq!(bash("cd /a && cd /b && rm -rf x"), "rm -rf");
        assert_eq!(bash("echo \"hi\""), "echo 'hi'");
        assert_eq!(bash(""), "");
        assert_eq!(bash("cd"), "cd");
    }

    /// 01M57XY3VP35QDP2KK88ERF82M.
    #[test]
    fn the_head_of_another_tool_is_the_path_or_the_first_field() {
        assert_eq!(head("Write", &json!({"file_path": "/a"})), "/a");
        assert_eq!(head("Glob", &json!({"path": "/b", "pattern": "*"})), "/b");
        assert_eq!(
            head("mcp__riff__post", &json!({"body": "hi\nthere"})),
            "hi there"
        );
        assert_eq!(head("X", &json!({"n": 3})), "3");
        assert_eq!(head("X", &json!({"n": "y".repeat(100)})).len(), HEAD_MAX);
    }

    /// 01M57XY3VP35QDP2KK88ERF82M.
    #[test]
    fn the_role_names_a_subagent() {
        let me: SessionUri = "riff://ann@heron/acme/app?session=a1b2c3d4e5"
            .parse()
            .unwrap();
        let plain = Denial::of(
            &input(json!({"tool_name": "Bash", "tool_input": {"command": "ls"}})),
            &Scrub::default(),
            true,
            0,
        );
        assert!(plain.worker);
        assert!(!plain.subagent);
        assert!(line(&plain, &Where::of(&me, Role::Worker, None)).contains(" role=worker "));
        let sub = Denial::of(
            &input(json!({"tool_name": "Bash", "agent_id": "ag1"})),
            &Scrub::default(),
            false,
            0,
        );
        assert!(sub.subagent);
        let text = line(&sub, &Where::of(&me, Role::Session, Some("issue-3")));
        assert!(text.contains(" role=session+subagent "), "{text}");
        assert!(text.contains(" session=a1b2c3d4 claim=issue-3: "), "{text}");
        for role in [Role::Lead, Role::Worker, Role::Container, Role::Session] {
            let parsed = Record::parse(&line(&plain, &Where::of(&me, role, None)), 0).unwrap();
            assert_eq!(parsed.tag("role"), role.name());
        }
    }

    /// 01M57XY3XXCKBXMVPFBY0C2KDM: a known secret never reaches the
    /// record, in the text, in the head or in the tool.
    #[test]
    fn the_scrub_hides_each_secret() {
        let secret = "gho_Zx9secretValue";
        let scrub = Scrub::new([secret.to_owned()]);
        let denial = Denial::of(
            &input(json!({
                "tool_name": "Bash",
                "tool_input": {"command": format!("{secret} push && curl -H 'Bearer {secret}'")},
                "reason": "[Credential Leak]",
                "mcp_server": secret,
            })),
            &scrub,
            false,
            0,
        );
        let me: SessionUri = "riff://ann@heron/acme/app?session=a1".parse().unwrap();
        let text = line(&denial, &Where::of(&me, Role::Lead, None));
        assert!(!text.contains(secret), "{text}");
        assert!(text.contains(crate::run::HIDDEN), "{text}");
        assert_eq!(denial.reason, "credential-leak");
    }

    /// 01M57XY3VP35QDP2KK88ERF82M: the text is one line of at most
    /// [`TEXT_MAX`] characters.
    #[test]
    fn the_text_is_one_short_line() {
        let long = format!("echo {}\nrm x", "é".repeat(400));
        let denial = Denial::of(
            &input(json!({"tool_name": "Bash", "tool_input": {"command": long}})),
            &Scrub::default(),
            false,
            0,
        );
        assert_eq!(denial.text.chars().count(), TEXT_MAX);
        assert!(!denial.text.contains('\n'));
        let other = Denial::of(
            &input(json!({"tool_name": "Edit", "tool_input": {"file_path": "/a"}})),
            &Scrub::default(),
            false,
            0,
        );
        assert_eq!(other.text, r#"{"file_path":"/a"}"#);
    }

    /// 01M57XY42CNR16M5G024PWKANV: when riff-server does not answer, the
    /// send keeps each denial in the local file, and the next send posts
    /// it first.
    #[tokio::test]
    async fn the_local_file_keeps_the_denials_when_the_server_does_not_answer() {
        let dir = tempfile::tempdir().unwrap();
        let queue = Queue::in_dir(dir.path());
        let denial = |at_ms| Denial {
            at_ms,
            reason: "r".into(),
            tool: "Bash".into(),
            head: "ls".into(),
            text: "ls".into(),
            worker: false,
            subagent: false,
        };
        queue.keep(&[denial(1)]).unwrap();
        let api = Api::new("http://127.0.0.1:9").one_try(Duration::from_millis(300));
        let me: SessionUri = "riff://ann@heron/acme/app?session=a1".parse().unwrap();
        assert!(send_here(&api, &me, &queue).await.is_err());
        queue.keep(&[denial(2)]).unwrap();
        let kept = queue.take();
        assert_eq!(kept.iter().map(|d| d.at_ms).collect::<Vec<_>>(), [1, 2]);
        assert!(queue.take().is_empty());
    }

    #[test]
    fn a_broken_line_of_the_local_file_goes() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(QUEUE), "not json\n").unwrap();
        assert!(Queue::in_dir(dir.path()).take().is_empty());
        assert!(!dir.path().join(QUEUE).exists());
    }

    /// 01M57XY44K2N3PS9F8NQ6SZQP9.
    #[test]
    fn the_report_shows_the_last_records() {
        let bodies: Vec<String> = (0..25)
            .map(|i| {
                format!(
                    "denied reason=r{} tool=Bash head=\"ls\" role=lead: ls {i}",
                    i % 2
                )
            })
            .collect();
        let records = records(
            bodies
                .iter()
                .enumerate()
                .map(|(i, b)| (i as u64, b.as_str())),
            0,
        );
        let text = report(&records, "in the thread denials");
        assert!(text.contains("By reason:\n  13  r0\n  12  r1\n"), "{text}");
        assert!(text.contains("Last 20:\n"), "{text}");
        assert!(text.contains(": ls 24\n"), "{text}");
        assert!(!text.contains(": ls 4\n"), "{text}");
    }
}
