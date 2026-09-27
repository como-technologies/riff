//! `riff tokens`: the token use of riff in Claude Code transcripts.
//!
//! # Design
//!
//! Claude Code writes one transcript for each session: a JSON Lines
//! file `SESSION.jsonl` in a project folder of `~/.claude/projects`.
//! The folder name is the directory of the session, with each
//! character that is not a letter or a digit changed to `-` (see
//! [`project_name`]). When a session moves to a worktree, its
//! transcript goes on in the folder of that worktree, with the same
//! file name. So [`sessions`] joins the files of one session ID into
//! one session (01M3JCFE44P47XAZ1STAM5M6JV).
//!
//! [`measure`] reads the rows of one session in order and counts:
//!
//! | Number | From |
//! |---|---|
//! | requests, input, output | the `usage` of each assistant row, once for each `requestId` (01M3JCFE67QMVGZHPWRGJ9Y83D) |
//! | requests that call only riff, or riff and other tools | the tool calls of each request (01M3JCFE8AECR4YPSZ1AHC1EAZ) |
//! | riff in context | the riff text in the context before each request, summed (01M3JCFEAGSJESRK7C43SQGTYV) |
//! | the riff skill | the text of the skill when it loads (01M3JCFECNFS828V7RAF9ZEG95) |
//! | wakes | the `riff wakes` notifications (01M3JCFEEVTKYS1CR45EBVW77R) |
//! | riff tool results | the result of each riff tool call (01M3JCFEGZWQ1SC76VF7DA6HX9) |
//!
//! A token of riff text is 4 characters. The transcript has no token
//! count for one part of the context. The rules are the same as the
//! script of the #60 baseline, so a new wave compares with it. The
//! length of a tool input is the length of its JSON as Python's
//! `json.dumps` writes it (see [`py_json`]).
//!
//! A [`Window`] limits the count to a time (01M3JCFENCPP20TJRYS505YER6).
//! The rows before the window still build the context. The rows after
//! it are left out.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use serde::Serialize;
use serde_json::Value;

/// The name prefix of each riff MCP tool in Claude Code.
pub const RIFF_TOOL: &str = "mcp__plugin_riff_riff__";

/// The characters of riff text that count as one token.
const CHARS_PER_TOKEN: u64 = 4;

/// A UTC time, to the nanosecond. It reads `2026-09-27`,
/// `2026-09-27T18:30:11Z` and `2026-09-27T18:30:11.123Z`: the form of
/// the transcript timestamps.
///
/// ```
/// use riff::tokens::Time;
///
/// let day: Time = "2026-09-27".parse()?;
/// let second: Time = "2026-09-27T18:30:11Z".parse()?;
/// let later: Time = "2026-09-27T18:30:11.123Z".parse()?;
/// assert!(day < second && second < later);
/// assert!("27/09/2026".parse::<Time>().is_err());
/// # Ok::<(), riff::tokens::BadTime>(())
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Time {
    /// The digits `YYYYMMDDHHMMSS` as one number.
    second: u64,
    nanos: u32,
}

/// A time that [`Time`] cannot read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BadTime(String);

impl fmt::Display for BadTime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:?} is not a UTC time. Use 2026-09-27 or 2026-09-27T18:30:11Z",
            self.0
        )
    }
}

impl std::error::Error for BadTime {}

impl FromStr for Time {
    type Err = BadTime;

    fn from_str(s: &str) -> Result<Self, BadTime> {
        let bad = || BadTime(s.to_owned());
        let t = s.strip_suffix('Z').unwrap_or(s);
        let (date, clock) = match t.split_once(['T', ' ']) {
            Some((date, clock)) => (date, clock),
            None => (t, "00:00:00"),
        };
        let (clock, fraction) = clock.split_once('.').unwrap_or((clock, ""));
        let digits = |part: &str, sep: char, n: usize| -> Option<String> {
            let fields: Vec<&str> = part.split(sep).collect();
            (fields.len() == n
                && fields
                    .iter()
                    .all(|f| !f.is_empty() && f.bytes().all(|b| b.is_ascii_digit())))
            .then(|| fields.concat())
        };
        let date = digits(date, '-', 3)
            .filter(|d| d.len() == 8)
            .ok_or_else(bad)?;
        let clock = digits(clock, ':', 3)
            .filter(|c| c.len() == 6)
            .ok_or_else(bad)?;
        if !fraction.bytes().all(|b| b.is_ascii_digit()) {
            return Err(bad());
        }
        let nanos = format!("{:0<9}", &fraction[..fraction.len().min(9)]);
        Ok(Time {
            second: format!("{date}{clock}").parse().map_err(|_| bad())?,
            nanos: nanos.parse().map_err(|_| bad())?,
        })
    }
}

/// The time to count. Both ends are optional.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Window {
    /// Count from this time on.
    pub since: Option<Time>,
    /// Count up to this time.
    pub until: Option<Time>,
}

impl Window {
    /// True when a row at `time` comes after the window. A row with no
    /// time is never after it.
    ///
    /// ```
    /// use riff::tokens::Window;
    ///
    /// let w = Window { since: None, until: Some("2026-09-27T18:30:11Z".parse()?) };
    /// assert!(w.after(Some("2026-09-27T18:30:12Z".parse()?)));
    /// assert!(!w.after(Some("2026-09-27T18:30:11Z".parse()?)));
    /// assert!(!w.after(None));
    /// # Ok::<(), riff::tokens::BadTime>(())
    /// ```
    pub fn after(&self, time: Option<Time>) -> bool {
        matches!((self.until, time), (Some(until), Some(t)) if t > until)
    }

    /// True when a row at `time` counts: it is not before the window.
    pub fn counts(&self, time: Option<Time>) -> bool {
        !matches!((self.since, time), (Some(since), Some(t)) if t < since)
    }
}

/// The riff tool results of one tool.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Results {
    /// The number of results.
    pub calls: u64,
    /// Their size in tokens.
    pub tokens: u64,
}

/// The numbers of one session, or of all sessions (see [`Measure::total`]).
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Measure {
    /// The session ID, or `total`.
    pub session: String,
    /// The API requests.
    pub requests: u64,
    /// The input tokens of the requests: new, cache write and cache read.
    pub input: u64,
    /// The output tokens of the requests.
    pub output: u64,
    /// The mean input of a request: the mean size of the context.
    pub mean_context: u64,
    /// The requests that call only riff tools.
    pub only_riff_requests: u64,
    /// The input of those requests.
    pub only_riff_input: u64,
    /// The requests that call riff tools and other tools.
    pub riff_and_other_requests: u64,
    /// The input of those requests.
    pub riff_and_other_input: u64,
    /// The riff text in the context of each request, in tokens, summed
    /// over the requests.
    pub riff_in_context: u64,
    /// The size of the riff skill in tokens.
    pub skill: u64,
    /// The wakes that came to the session.
    pub wakes: u64,
    /// The wakes that started a turn.
    pub wakes_that_start_a_turn: u64,
    /// The wakes that came in a turn.
    pub wakes_in_a_turn: u64,
    /// The requests of the turns that a wake started.
    pub wake_turn_requests: u64,
    /// The input of those requests.
    pub wake_turn_input: u64,
    /// The context compactions.
    pub compactions: u64,
    /// The riff tool calls, by tool.
    pub riff_calls: BTreeMap<String, u64>,
    /// The riff tool results, by tool.
    pub riff_results: BTreeMap<String, Results>,
}

impl Measure {
    /// `part` as a share of the input.
    ///
    /// ```
    /// let m = riff::tokens::Measure { input: 200, ..Default::default() };
    /// assert_eq!(m.share(50), 0.25);
    /// assert_eq!(riff::tokens::Measure::default().share(50), 0.0);
    /// ```
    pub fn share(&self, part: u64) -> f64 {
        if self.input == 0 {
            0.0
        } else {
            part as f64 / self.input as f64
        }
    }

    /// The riff tool results of all tools, in tokens.
    pub fn riff_result_tokens(&self) -> u64 {
        self.riff_results.values().map(|r| r.tokens).sum()
    }

    /// The sum of each session. The skill is the largest one.
    ///
    /// ```
    /// use riff::tokens::Measure;
    ///
    /// let a = Measure { requests: 1, input: 100, skill: 7, ..Default::default() };
    /// let b = Measure { requests: 3, input: 500, skill: 5, ..Default::default() };
    /// let t = Measure::total(&[a, b]);
    /// assert_eq!((t.session.as_str(), t.requests, t.mean_context, t.skill), ("total", 4, 150, 7));
    /// ```
    pub fn total(all: &[Measure]) -> Measure {
        let mut t = Measure {
            session: "total".into(),
            ..Default::default()
        };
        for m in all {
            t.requests += m.requests;
            t.input += m.input;
            t.output += m.output;
            t.only_riff_requests += m.only_riff_requests;
            t.only_riff_input += m.only_riff_input;
            t.riff_and_other_requests += m.riff_and_other_requests;
            t.riff_and_other_input += m.riff_and_other_input;
            t.riff_in_context += m.riff_in_context;
            t.skill = t.skill.max(m.skill);
            t.wakes += m.wakes;
            t.wakes_that_start_a_turn += m.wakes_that_start_a_turn;
            t.wakes_in_a_turn += m.wakes_in_a_turn;
            t.wake_turn_requests += m.wake_turn_requests;
            t.wake_turn_input += m.wake_turn_input;
            t.compactions += m.compactions;
            for (tool, n) in &m.riff_calls {
                *t.riff_calls.entry(tool.clone()).or_default() += n;
            }
            for (tool, r) in &m.riff_results {
                let e = t.riff_results.entry(tool.clone()).or_default();
                e.calls += r.calls;
                e.tokens += r.tokens;
            }
        }
        t.mean_context = t.input / t.requests.max(1);
        t
    }
}

/// The report of `riff tokens --json` (01M3JCFEK6Q4YXXJ0ERMJ2KS65).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Report {
    /// Each session, in the order that they started.
    pub sessions: Vec<Measure>,
    /// The sum of the sessions.
    pub total: Measure,
}

impl Report {
    /// The report of `sessions`.
    pub fn new(sessions: Vec<Measure>) -> Report {
        let total = Measure::total(&sessions);
        Report { sessions, total }
    }
}

/// The transcript of one session: its rows in order.
#[derive(Debug, Clone, PartialEq)]
pub struct Session {
    /// The session ID: the name of the transcript file.
    pub id: String,
    /// The rows of the transcript.
    pub rows: Vec<Value>,
}

/// The name of the project folder of `dir`: each character that is not
/// a letter or a digit becomes `-`.
///
/// ```
/// use std::path::Path;
///
/// let main = Path::new("/home/mike/src/como-technologies/riff");
/// assert_eq!(riff::tokens::project_name(main), "-home-mike-src-como-technologies-riff");
/// let wt = main.join(".claude/worktrees/issue-62");
/// assert_eq!(
///     riff::tokens::project_name(&wt),
///     "-home-mike-src-como-technologies-riff--claude-worktrees-issue-62"
/// );
/// ```
pub fn project_name(dir: &Path) -> String {
    dir.to_string_lossy()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

/// The Claude Code projects folder: `$CLAUDE_CONFIG_DIR/projects`, or
/// `~/.claude/projects`.
pub fn projects_dir() -> Option<PathBuf> {
    let config = std::env::var_os("CLAUDE_CONFIG_DIR")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".claude")))?;
    Some(config.join("projects"))
}

/// The transcript files of a repository whose main worktree is `main`:
/// the files of its project folder and of the folder of each worktree
/// in `.claude/worktrees`.
pub fn repo_transcripts(projects: &Path, main: &Path) -> std::io::Result<Vec<PathBuf>> {
    let base = project_name(main);
    let worktrees = format!("{base}--claude-worktrees-");
    let mut files = Vec::new();
    for dir in std::fs::read_dir(projects)? {
        let dir = dir?;
        let name = dir.file_name().to_string_lossy().into_owned();
        if name != base && !name.starts_with(&worktrees) {
            continue;
        }
        for file in std::fs::read_dir(dir.path())? {
            let path = file?.path();
            if path.extension().is_some_and(|e| e == "jsonl") && path.is_file() {
                files.push(path);
            }
        }
    }
    files.sort();
    Ok(files)
}

/// Reads the transcript `files` as sessions. The files with the same
/// name are one session, in the order of their first row. A row that
/// is in two files counts once. A line that is not JSON is left out.
/// The sessions come in the order that they started.
pub fn sessions(files: &[PathBuf]) -> std::io::Result<Vec<Session>> {
    let mut parts: BTreeMap<String, Vec<Vec<Value>>> = BTreeMap::new();
    for file in files {
        let text = std::fs::read_to_string(file)?;
        let rows: Vec<Value> = text
            .lines()
            .filter_map(|l| serde_json::from_str(l).ok())
            .collect();
        let id = file
            .file_stem()
            .map_or_else(String::new, |s| s.to_string_lossy().into_owned());
        parts.entry(id).or_default().push(rows);
    }
    let mut out: Vec<Session> = parts
        .into_iter()
        .map(|(id, mut parts)| {
            parts.sort_by_key(|rows| first_time(rows));
            let mut seen = HashSet::new();
            let rows = parts
                .into_iter()
                .flatten()
                .filter(|r| match r.get("uuid").and_then(Value::as_str) {
                    Some(uuid) => seen.insert(uuid.to_owned()),
                    None => true,
                })
                .collect();
            Session { id, rows }
        })
        .collect();
    out.sort_by_key(|s| first_time(&s.rows));
    Ok(out)
}

fn time_of(row: &Value) -> Option<Time> {
    row.get("timestamp")?.as_str()?.parse().ok()
}

fn first_time(rows: &[Value]) -> Option<Time> {
    rows.iter().find_map(time_of)
}

/// A value as Python's `json.dumps` writes it: `", "` and `": "`
/// between items, and each character that is not printable ASCII as
/// `\uXXXX`. The #60 script counts tool inputs this way.
///
/// ```
/// use serde_json::json;
///
/// // `~` stands for a backslash in the expected text.
/// let e_acute = char::from_u32(0xe9).unwrap().to_string();
/// let v = json!({"a": [1, e_acute, null], "b": "x\ny"});
/// let expected = r#"{"a": [1, "~u00e9", null], "b": "x~ny"}"#.replace('~', "\\");
/// assert_eq!(riff::tokens::py_json(&v), expected);
/// let note = char::from_u32(0x1f3b5).unwrap().to_string();
/// let expected = r#""~ud83c~udfb5""#.replace('~', "\\");
/// assert_eq!(riff::tokens::py_json(&json!(note)), expected);
/// ```
pub fn py_json(v: &Value) -> String {
    let mut out = String::new();
    write_py(v, &mut out);
    out
}

fn write_py(v: &Value, out: &mut String) {
    match v {
        Value::String(s) => write_py_str(s, out),
        Value::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                write_py(item, out);
            }
            out.push(']');
        }
        Value::Object(map) => {
            out.push('{');
            for (i, (k, item)) in map.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                write_py_str(k, out);
                out.push_str(": ");
                write_py(item, out);
            }
            out.push('}');
        }
        other => out.push_str(&other.to_string()),
    }
}

fn write_py_str(s: &str, out: &mut String) {
    use std::fmt::Write;
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            ' '..='~' => out.push(c),
            _ => {
                let mut units = [0u16; 2];
                for u in c.encode_utf16(&mut units) {
                    let _ = write!(out, "\\u{u:04x}");
                }
            }
        }
    }
    out.push('"');
}

/// The text of a message content: its text, the text of its tool
/// results, and the JSON of its tool calls.
fn text_of(content: &Value) -> String {
    match content {
        Value::String(s) => s.clone(),
        Value::Array(items) => items
            .iter()
            .map(|c| match c.get("type").and_then(Value::as_str) {
                Some("text") => c.get("text").and_then(Value::as_str).unwrap_or("").into(),
                Some("tool_result") => text_of(c.get("content").unwrap_or(&Value::Null)),
                Some("tool_use") => py_json(c.get("input").unwrap_or(&Value::Null)),
                _ => String::new(),
            })
            .collect(),
        _ => String::new(),
    }
}

fn chars(s: &str) -> u64 {
    s.chars().count() as u64
}

fn tok(chars: u64) -> u64 {
    chars / CHARS_PER_TOKEN
}

fn str_of<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(Value::as_str).unwrap_or("")
}

/// The short name of a riff call: the tool without its prefix, or the
/// `riff` command that Bash runs, for example `riff watch`.
fn riff_call(name: &str, command: &str) -> Option<String> {
    if let Some(tool) = name.strip_prefix(RIFF_TOOL) {
        return Some(tool.to_owned());
    }
    (name == "Bash" && command.starts_with("riff ")).then(|| {
        command
            .split_whitespace()
            .take(2)
            .collect::<Vec<_>>()
            .join(" ")
    })
}

/// Measures one session in `window`.
pub fn measure(session: &Session, window: &Window) -> Measure {
    let mut m = Measure {
        session: session.id.clone(),
        ..Default::default()
    };
    let mut tool_names: HashMap<String, String> = HashMap::new();
    let mut watch_ids: HashSet<String> = HashSet::new();
    let mut seen: HashSet<String> = HashSet::new();
    let mut inputs: HashMap<String, u64> = HashMap::new();
    let mut riff_requests: HashMap<String, u64> = HashMap::new();
    let mut other_requests: HashSet<String> = HashSet::new();
    let mut result_chars: BTreeMap<String, (u64, u64)> = BTreeMap::new();
    // The riff text in the context now, in characters.
    let mut riff_chars: u64 = 0;
    let mut wake_turn = false;
    let null = Value::Null;
    for d in &session.rows {
        let time = time_of(d);
        if window.after(time) {
            continue;
        }
        let counts = window.counts(time);
        let kind = str_of(d, "type");
        let msg = d.get("message").unwrap_or(&null);
        if d.get("isCompactSummary").and_then(Value::as_bool) == Some(true)
            || (kind == "system" && str_of(d, "subtype") == "compact_boundary")
        {
            riff_chars = 0;
            if counts {
                m.compactions += 1;
            }
        }
        match kind {
            "assistant" => {
                let rid = d.get("requestId").and_then(Value::as_str);
                if let Some(rid) = rid
                    && seen.insert(rid.to_owned())
                    && counts
                {
                    let u = msg.get("usage").unwrap_or(&null);
                    let n = |k: &str| u.get(k).and_then(Value::as_u64).unwrap_or(0);
                    let input = n("input_tokens")
                        + n("cache_creation_input_tokens")
                        + n("cache_read_input_tokens");
                    inputs.insert(rid.to_owned(), input);
                    m.requests += 1;
                    m.input += input;
                    m.output += n("output_tokens");
                    m.riff_in_context += tok(riff_chars);
                    if wake_turn {
                        m.wake_turn_requests += 1;
                        m.wake_turn_input += input;
                    }
                }
                let content = msg.get("content").and_then(Value::as_array);
                for c in content.into_iter().flatten() {
                    if str_of(c, "type") != "tool_use" {
                        continue;
                    }
                    let name = str_of(c, "name");
                    let input = c.get("input").unwrap_or(&null);
                    let command = str_of(input, "command");
                    let call = riff_call(name, command);
                    if let Some(rid) = rid
                        && let Some(&request_input) = inputs.get(rid)
                    {
                        if call.is_some() {
                            riff_requests.insert(rid.to_owned(), request_input);
                        } else {
                            other_requests.insert(rid.to_owned());
                        }
                    }
                    if let Some(call) = call
                        && counts
                    {
                        *m.riff_calls.entry(call).or_default() += 1;
                    }
                    let id = str_of(c, "id").to_owned();
                    tool_names.insert(id.clone(), name.to_owned());
                    let len = chars(&py_json(input));
                    if name.starts_with(RIFF_TOOL) {
                        riff_chars += len;
                    }
                    if name == "Bash" && command.contains("riff watch") {
                        watch_ids.insert(id);
                        riff_chars += len;
                    }
                    if name == "Skill" && str_of(input, "skill").contains("riff") {
                        riff_chars += len;
                    }
                }
            }
            "user" => {
                let content = msg.get("content").unwrap_or(&null);
                let has_text = content.is_string()
                    || content
                        .as_array()
                        .is_some_and(|a| a.iter().any(|c| str_of(c, "type") == "text"));
                if has_text {
                    // A new turn: a person, or a notification of an idle session.
                    let txt = text_of(content);
                    let notice = txt.contains("<task-notification>");
                    let is_wake = notice && txt.contains("riff wakes");
                    if is_wake {
                        if counts {
                            m.wakes_that_start_a_turn += 1;
                        }
                        riff_chars += chars(&txt);
                    }
                    let human = d
                        .get("origin")
                        .is_some_and(|o| str_of(o, "kind") == "human");
                    if notice || human {
                        wake_turn = is_wake;
                    }
                    if txt.contains("Base directory for this skill") && txt.contains("# Riff") {
                        riff_chars += chars(&txt);
                        m.skill = tok(chars(&txt));
                    }
                }
                for c in content.as_array().into_iter().flatten() {
                    if str_of(c, "type") != "tool_result" {
                        continue;
                    }
                    let id = str_of(c, "tool_use_id");
                    let name = tool_names.get(id).map_or("", String::as_str);
                    let short = match name.strip_prefix(RIFF_TOOL) {
                        Some(tool) => tool,
                        None if watch_ids.contains(id) => "watch",
                        None => continue,
                    };
                    let n = chars(&text_of(c.get("content").unwrap_or(&null)));
                    riff_chars += n;
                    if counts {
                        let e = result_chars.entry(short.to_owned()).or_default();
                        e.0 += 1;
                        e.1 += n;
                    }
                }
            }
            "attachment" => {
                let txt = match d.get("attachment") {
                    Some(a) => py_json(a),
                    None => "{}".into(),
                };
                if txt.contains("riff wakes") {
                    if counts {
                        m.wakes_in_a_turn += 1;
                    }
                    riff_chars += chars(&txt);
                } else if txt.contains("riff: this session is") {
                    riff_chars += chars(&txt);
                }
            }
            "queue-operation"
                if str_of(d, "operation") == "enqueue"
                    && str_of(d, "content").contains("riff wakes")
                    && counts =>
            {
                m.wakes += 1;
            }
            _ => {}
        }
    }
    for (rid, input) in &riff_requests {
        if other_requests.contains(rid) {
            m.riff_and_other_requests += 1;
            m.riff_and_other_input += input;
        } else {
            m.only_riff_requests += 1;
            m.only_riff_input += input;
        }
    }
    m.riff_results = result_chars
        .into_iter()
        .map(|(tool, (calls, n))| {
            (
                tool,
                Results {
                    calls,
                    tokens: tok(n),
                },
            )
        })
        .collect();
    m.mean_context = m.input / m.requests.max(1);
    m
}

/// Measures each session in `window`, and leaves out the sessions with
/// no request in it.
pub fn report(sessions: &[Session], window: &Window) -> Report {
    Report::new(
        sessions
            .iter()
            .map(|s| measure(s, window))
            .filter(|m| m.requests > 0)
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn session(rows: Vec<Value>) -> Session {
        Session {
            id: "s".into(),
            rows,
        }
    }

    fn assistant(rid: &str, time: &str, input: u64, tools: Value) -> Value {
        json!({
            "type": "assistant", "requestId": rid, "timestamp": time,
            "message": {"usage": {"input_tokens": input, "output_tokens": 1}, "content": tools}
        })
    }

    #[test]
    fn a_request_counts_once() {
        let rows = vec![
            assistant("r1", "2026-09-27T10:00:00Z", 100, json!([])),
            assistant("r1", "2026-09-27T10:00:00Z", 100, json!([])),
        ];
        let m = measure(&session(rows), &Window::default());
        assert_eq!((m.requests, m.input, m.output), (1, 100, 1));
    }

    #[test]
    fn a_request_with_riff_and_bash_is_not_only_riff() {
        let riff = json!({"type": "tool_use", "id": "t1", "name": "mcp__plugin_riff_riff__read", "input": {}});
        let bash =
            json!({"type": "tool_use", "id": "t2", "name": "Bash", "input": {"command": "ls"}});
        let rows = vec![
            assistant("r1", "2026-09-27T10:00:00Z", 100, json!([riff])),
            assistant("r1", "2026-09-27T10:00:00Z", 100, json!([bash])),
            assistant("r2", "2026-09-27T10:00:01Z", 50, json!([riff])),
        ];
        let m = measure(&session(rows), &Window::default());
        assert_eq!(
            (m.riff_and_other_requests, m.riff_and_other_input),
            (1, 100)
        );
        assert_eq!((m.only_riff_requests, m.only_riff_input), (1, 50));
        assert_eq!(m.riff_calls["read"], 2);
    }

    #[test]
    fn the_window_counts_only_its_rows_but_keeps_the_context() {
        let call = json!({"type": "tool_use", "id": "t1", "name": "mcp__plugin_riff_riff__read", "input": {}});
        let result = json!({"type": "user", "timestamp": "2026-09-27T10:00:01Z",
            "message": {"content": [{"type": "tool_result", "tool_use_id": "t1", "content": "x".repeat(400)}]}});
        let rows = vec![
            assistant("r1", "2026-09-27T10:00:00Z", 100, json!([call])),
            result,
            assistant("r2", "2026-09-27T11:00:00Z", 200, json!([])),
            assistant("r3", "2026-09-27T12:00:00Z", 300, json!([])),
        ];
        let window = Window {
            since: Some("2026-09-27T11:00:00Z".parse().unwrap()),
            until: Some("2026-09-27T11:30:00Z".parse().unwrap()),
        };
        let m = measure(&session(rows), &window);
        assert_eq!((m.requests, m.input), (1, 200));
        assert!(m.riff_calls.is_empty() && m.riff_results.is_empty());
        // "{}" (0 tokens) and 400 characters of result: 100 tokens.
        assert_eq!(m.riff_in_context, 100);
    }

    #[test]
    fn a_compaction_empties_the_riff_context() {
        let call = json!({"type": "tool_use", "id": "t1", "name": "mcp__plugin_riff_riff__read", "input": {}});
        let result = json!({"type": "user",
            "message": {"content": [{"type": "tool_result", "tool_use_id": "t1", "content": "x".repeat(400)}]}});
        let rows = vec![
            assistant("r1", "2026-09-27T10:00:00Z", 100, json!([call])),
            result,
            json!({"type": "system", "subtype": "compact_boundary"}),
            assistant("r2", "2026-09-27T11:00:00Z", 200, json!([])),
        ];
        let m = measure(&session(rows), &Window::default());
        assert_eq!((m.riff_in_context, m.compactions), (0, 1));
    }

    #[test]
    fn a_bad_time_is_refused() {
        for bad in [
            "",
            "2026-09",
            "2026-09-27T18:30",
            "2026-09-27Tab:cd:ef",
            "2026-09-27T18:30:11.x",
        ] {
            assert!(bad.parse::<Time>().is_err(), "{bad}");
        }
    }
}
