//! The trace of a call that left no record: one log line.
//!
//! # Design
//!
//! Each command leaves one trace (01M3X4Z62RJREQ5H8F18Y85T6V). A command that made
//! records has them in the log, with its caller and its kind in each
//! envelope ([`riff_core::record::Record`]). Each other command gets
//! one log line of `riff-server`. So a person finds who did what in the
//! log, and who was refused in the log lines.
//!
//! ```mermaid
//! flowchart TD
//!     C[call] --> T{token layer}
//!     T -->|refused| M{the limit of rate}
//!     M -->|a line is free| D["line: denied<br/>named, path, code"]
//!     M -->|over the limit| X["no line: the count goes up"]
//!     X -->|the window ends| O["line: dropped<br/>count"]
//!     T -->|a signal or a query| N[no line]
//!     T -->|a command| E[the entry in the queue]
//!     E --> W{the writer}
//!     W -->|the write of the chunk fails| F["line: failed, ERROR"]
//!     E -->|the server stops first| FW["line: failed, WARNING"]
//!     W -->|records| L[no line: the records are the trace]
//!     W -->|refused| R["line: refused<br/>code, reason"]
//!     W -->|accepted, no record| NC["line: no_change"]
//! ```
//!
//! A line has the format of each other log line ([`crate::logline`]):
//! `severity`, `time`, `message` and `target`, then these fields. The
//! target is [`TARGET`], and the message is the result.
//!
//! | `result` | When | Severity | Made by |
//! |---|---|---|---|
//! | `refused` | `permits` or `handle` refused the command. | `INFO` | the writer |
//! | `no_change` | `handle` accepted the command, and it made no record. | `INFO` | the writer |
//! | `failed` | The write of the chunk of the command failed, and the instance stopped. | `ERROR` | the writer |
//! | `failed` | The command waited in the queue when the server stopped, for example for a lost lease. The line has the `reason` of the stop. | `WARNING` | the engine |
//! | `denied` | The token layer refused the call: a command, a query or a signal. | `INFO` | the token layer |
//! | `dropped` | A window of the limit of rate ended, and the token layer did not write some `denied` lines in it. | `WARNING` | the token layer |
//!
//! | Field | In | Holds |
//! |---|---|---|
//! | `caller` | `refused`, `no_change`, `failed` | The caller, as `by` in a record: `{"session":"mike/84cf"}`. |
//! | `key` | `refused`, `no_change`, `failed` | The thumbprint of the device key of the token. It is not a secret. A call with no token has none. |
//! | `command` | `refused`, `no_change`, `failed` | The kind of the command. |
//! | `code`, `reason` | `refused` | The code of the refusal ([`Code`](crate::state::Code)) and its reason as text. The line of a command of the people has no `reason`. |
//! | `reason` | `failed` with `WARNING` | Why the server stopped. |
//! | `named` | `denied` | The caller that the call named: the `me` of the body, or the `uri` of the query. |
//! | `proved` | `denied` | `false`: no token proved the name. |
//! | `named_cut` | `denied` | `true` when the name was longer than [`NAMED_MAX`] characters: the line has its start. |
//! | `path` | `denied` | The path of the call, in the place of `command`. |
//! | `code` | `denied` | [`DeniedCode`]. |
//! | `count` | `dropped` | The number of `denied` lines that the server did not write in the window. |
//!
//! # Rules
//!
//! - A line never holds the body of a post, a token or a key
//!   (01M3X4Z675D0ZQX93E93F3M8FA). The reason of a refusal names items, threads and
//!   sessions only.
//! - A line never holds an email (01M3XA87CJHCGZX283ZQAFKARZ). The
//!   people are personal data: an email is in a record, and in a reply
//!   to a member. So the line of a refused command of the people has
//!   its code and no reason, and the line of a sign-in names its USER:
//!   `{"sign_in":"mike"}`.
//! - A command with records gets no line. A signal and a query that the
//!   token layer accepts get no line.
//! - The lines are best effort: a stop between the result and the line
//!   loses the line.
//! - The server reads the body of a call for `named` only after it
//!   refused the call, and at most [`BODY_MAX`] bytes of it.
//!
//! # The limits
//!
//! The cost of a refused call has a bound. [`Limit`] holds the count of
//! the `denied` lines. The engine holds one `Limit`, so the count is
//! one count for the whole server.
//!
//! - The read of the body for `named` takes at most [`BODY_TIME`]
//!   (01M3Z67B9RMVKY7TCXCG8HEZT4). After it, the line has no `named`.
//!   When the read stops before the end of the body (the time limit,
//!   or [`BODY_MAX`]), the reply has the header `connection: close`,
//!   and the server reads no more of the call.
//! - The server writes at most [`DENIED_MAX`] `denied` lines in each
//!   [`DENIED_INTERVAL`] (01M3Z67DZX9BC3TYF3PWGFGZJ7). A window starts at
//!   the first refused call after the end of the last window. Over the
//!   limit, the server writes no line and does not read the body. At
//!   the end of a window with such calls, it writes one line `dropped`
//!   with their `count`. A stop of the server ends the window
//!   ([`Limit::close`]), so no count is lost.
//! - The limits change only the lines. The reply to a refused call has
//!   the same status and the same text with a line and with no line.
//!
//! # Example
//!
//! ```
//! use riff_server::trace::Named;
//!
//! let named = Named::of_text("riff://mike@pangolin/como-technologies/riff?session=84cf");
//! assert_eq!(named.json().to_string(), r#"{"session":"mike/84cf"}"#);
//! assert!(!named.cut());
//!
//! // A name that is no URI is text. A long name is cut.
//! let long = Named::of_text(&"x".repeat(5_000));
//! assert_eq!(long.json()["text"].as_str().unwrap().len(), 200);
//! assert!(long.cut());
//! ```
//!
//! The limit of rate:
//!
//! ```
//! use riff_server::trace::{DENIED_MAX, Limit};
//!
//! let limit = Limit::default();
//! // The first lines of a window are free.
//! assert!((0..DENIED_MAX).all(|_| limit.take()));
//! // Each later call of the window gets no line. The limit counts it.
//! assert!(!limit.take());
//! assert!(!limit.take());
//! assert_eq!(limit.dropped(), 2);
//! // The end of the window writes the line `dropped` with the count.
//! limit.close();
//! assert_eq!(limit.dropped(), 0);
//! ```

use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use axum::extract::{Query, Request};
use riff_core::name::SessionUri;
use riff_core::record::By;
use serde_json::{Value, json};
use tokio::time::Instant;

use crate::state::{CommandKind, Refused};

/// The `target` of each line of a trace.
pub const TARGET: &str = "engine";

/// The most characters of a name in `named`. A longer name is cut.
pub const NAMED_MAX: usize = 200;

/// The most bytes of the body of a refused call that the server reads
/// to find the name.
pub const BODY_MAX: usize = 64 * 1024;

/// The most time that the server reads the body of a refused call to
/// find the name (01M3Z67B9RMVKY7TCXCG8HEZT4).
pub const BODY_TIME: Duration = Duration::from_secs(2);

/// The most `denied` lines that the server writes in one
/// [`DENIED_INTERVAL`] (01M3Z67DZX9BC3TYF3PWGFGZJ7).
pub const DENIED_MAX: u64 = 100;

/// The length of one window of the limit of rate of the `denied` lines
/// (01M3Z67DZX9BC3TYF3PWGFGZJ7).
pub const DENIED_INTERVAL: Duration = Duration::from_secs(10);

/// The result of a command with no record.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome<'a> {
    /// `permits` or `handle` refused the command.
    Refused(&'a Refused),
    /// The command is accepted, and it made no record.
    NoChange,
    /// The chunk of the command was not written: the write failed.
    Failed,
    /// The server stopped while the command waited in the queue. The
    /// text says why it stopped, for example a lost lease.
    Stopped(&'a str),
}

/// Who sent a command, for its line: the caller, the key of its token
/// and the kind of the command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Traced {
    pub caller: By,
    /// The thumbprint of the device key of the token, when a token
    /// proved the caller.
    pub key: Option<String>,
    pub command: CommandKind,
}

impl Traced {
    /// Writes the one line of a command with no record (01M3X4Z62RJREQ5H8F18Y85T6V).
    pub fn line(&self, outcome: Outcome<'_>) {
        let caller = self.caller.json().to_string();
        let caller = caller.as_str();
        let key = self.key.as_deref();
        let command = self.command.as_str();
        match outcome {
            Outcome::Refused(refused) => {
                // The reason of a command of the people can name an
                // email: its line has only the code
                // (01M3XA87CJHCGZX283ZQAFKARZ).
                let reason = (!self.command.of_people()).then_some(refused.reason.as_str());
                tracing::info!(
                    target: TARGET,
                    caller,
                    key,
                    command,
                    result = "refused",
                    code = refused.code.as_str(),
                    reason,
                    "refused"
                );
            }
            Outcome::NoChange => tracing::info!(
                target: TARGET,
                caller,
                key,
                command,
                result = "no_change",
                "no_change"
            ),
            Outcome::Failed => tracing::error!(
                target: TARGET,
                caller,
                key,
                command,
                result = "failed",
                "failed"
            ),
            Outcome::Stopped(why) => tracing::warn!(
                target: TARGET,
                caller,
                key,
                command,
                result = "failed",
                reason = why,
                "failed"
            ),
        }
    }
}

/// Why the token layer refused a call: the code of a `denied` line. It
/// is not a code of a refused command
/// ([`Code`](crate::state::Code)).
///
/// | Code | When | HTTP status |
/// |---|---|---|
/// | `no_token` | The call has no token. | 401 from the token check, 403 from the engine |
/// | `bad_token` | The server does not know the token, or the token expired. | 401 |
/// | `bad_proof` | The proof of the device key is refused, or the signature of a post. | 401 for a proof, 403 for a post |
/// | `not_you` | The token may not act as the `me` of the body. | 403 |
/// | `old_build` | The build of the client is too old or too new. | 409 |
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeniedCode {
    NoToken,
    BadToken,
    BadProof,
    NotYou,
    OldBuild,
}

impl DeniedCode {
    /// Each code.
    pub const ALL: [DeniedCode; 5] = [
        DeniedCode::NoToken,
        DeniedCode::BadToken,
        DeniedCode::BadProof,
        DeniedCode::NotYou,
        DeniedCode::OldBuild,
    ];

    /// The name of the code.
    pub fn as_str(self) -> &'static str {
        match self {
            DeniedCode::NoToken => "no_token",
            DeniedCode::BadToken => "bad_token",
            DeniedCode::BadProof => "bad_proof",
            DeniedCode::NotYou => "not_you",
            DeniedCode::OldBuild => "old_build",
        }
    }
}

/// A call that the token layer refused: the code for the line, and the
/// reason for the caller.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Denied {
    pub code: DeniedCode,
    pub reason: String,
}

impl Denied {
    pub fn new(code: DeniedCode, reason: impl Into<String>) -> Denied {
        Denied {
            code,
            reason: reason.into(),
        }
    }
}

/// The caller that a refused call named. No token proved it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Named {
    value: Value,
    cut: bool,
}

impl Named {
    /// The name of the session URI `me`, as `by` in a record.
    pub fn of(me: &SessionUri) -> Named {
        let who = me.who();
        let (class, name) = match who.session() {
            Some(_) => ("session", who.to_string()),
            None => ("person", who.user().to_owned()),
        };
        let (name, cut) = cut(&name);
        Named {
            value: json!({ class: name }),
            cut,
        }
    }

    /// The name of the text `me`: a session URI, or text that is no
    /// URI.
    pub fn of_text(me: &str) -> Named {
        match me.parse::<SessionUri>() {
            Ok(uri) => Named::of(&uri),
            Err(_) => {
                let (text, cut) = cut(me);
                Named {
                    value: json!({ "text": text }),
                    cut,
                }
            }
        }
    }

    /// The name as a JSON value, for the line.
    pub fn json(&self) -> &Value {
        &self.value
    }

    /// True when the name was longer than [`NAMED_MAX`] characters.
    pub fn cut(&self) -> bool {
        self.cut
    }

    /// Finds the name in a call that the server refused: the `uri` or
    /// the `me` of the query, else the `me` of a JSON body. It reads at
    /// most [`BODY_MAX`] bytes of the body, for at most [`BODY_TIME`]
    /// (01M3Z67B9RMVKY7TCXCG8HEZT4). A call with a longer body, with a
    /// slower body or with no name gives no name.
    pub async fn in_request(request: Request) -> Found {
        type Fields = std::collections::HashMap<String, String>;
        if let Ok(Query(query)) = Query::<Fields>::try_from_uri(request.uri())
            && let Some(me) = query.get("uri").or_else(|| query.get("me"))
        {
            return Found {
                named: Some(Named::of_text(me)),
                whole: true,
            };
        }
        let read = axum::body::to_bytes(request.into_body(), BODY_MAX);
        let Ok(Ok(bytes)) = tokio::time::timeout(BODY_TIME, read).await else {
            return Found {
                named: None,
                whole: false,
            };
        };
        let named = serde_json::from_slice::<Value>(&bytes)
            .ok()
            .and_then(|body| body.get("me")?.as_str().map(Named::of_text));
        Found { named, whole: true }
    }
}

/// What the server found in a call that it refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Found {
    /// The caller that the call named, when it named one.
    pub named: Option<Named>,
    /// False when the read of the body stopped before its end: at
    /// [`BODY_TIME`] or at [`BODY_MAX`]. The reply then closes the
    /// call.
    pub whole: bool,
}

/// The first [`NAMED_MAX`] characters of `text`, and true when it had
/// more.
fn cut(text: &str) -> (String, bool) {
    let mut chars = text.chars();
    let start: String = chars.by_ref().take(NAMED_MAX).collect();
    (start, chars.next().is_some())
}

/// Writes the one line of a call that the token layer refused
/// (01M3X4Z64ZNRD0G0F4JV1M64FN): the caller that the call named, marked as not proved,
/// the path and the code. It has no token, no key and no reason.
pub fn denied(path: &str, named: Option<&Named>, code: DeniedCode) {
    let name = named.map(|named| named.json().to_string());
    tracing::info!(
        target: TARGET,
        named = name.as_deref(),
        proved = named.map(|_| false),
        named_cut = named.and_then(|named| named.cut().then_some(true)),
        path,
        result = "denied",
        code = code.as_str(),
        "denied"
    );
}

/// Writes the one line of a window in which the server did not write
/// `count` `denied` lines (01M3Z67DZX9BC3TYF3PWGFGZJ7).
fn dropped(count: u64) {
    tracing::warn!(target: TARGET, count, result = "dropped", "dropped");
}

/// One window of the limit of rate.
#[derive(Debug, Default)]
struct Window {
    /// The end of the window. The limit has none before its first line.
    end: Option<Instant>,
    /// The `denied` lines of the window.
    written: u64,
    /// The `denied` lines that the server did not write in the window.
    dropped: u64,
}

impl Window {
    /// Ends the window: writes its line `dropped` when it has a count.
    fn close(&mut self) {
        let count = std::mem::take(&mut self.dropped);
        if count > 0 {
            dropped(count);
        }
    }
}

/// The limit of rate of the `denied` lines of one server
/// (01M3Z67DZX9BC3TYF3PWGFGZJ7). Clones share the count. See "The limits"
/// in the module docs.
#[derive(Clone, Debug, Default)]
pub struct Limit(Arc<Mutex<Window>>);

impl Limit {
    fn window(&self) -> MutexGuard<'_, Window> {
        // A panic while the lock is held leaves plain data behind; keep going.
        self.0.lock().unwrap_or_else(|poison| poison.into_inner())
    }

    /// Takes one `denied` line of the window. It gives false when the
    /// window has no line left: the caller writes no line, and the
    /// limit counts it.
    ///
    /// The first call after the end of a window starts the next window.
    /// The first call over the limit starts a timer that writes the
    /// line `dropped` at the end of the window. With no runtime, the
    /// next call after the window writes that line.
    pub fn take(&self) -> bool {
        let now = Instant::now();
        let mut window = self.window();
        let end = match window.end {
            Some(end) if now < end => end,
            _ => {
                window.close();
                let end = now + DENIED_INTERVAL;
                window.end = Some(end);
                window.written = 0;
                end
            }
        };
        if window.written < DENIED_MAX {
            window.written += 1;
            return true;
        }
        window.dropped += 1;
        if window.dropped == 1
            && let Ok(runtime) = tokio::runtime::Handle::try_current()
        {
            let limit = self.clone();
            runtime.spawn(async move {
                tokio::time::sleep_until(end).await;
                let mut window = limit.window();
                // A later call can have closed this window already.
                if window.end == Some(end) {
                    window.close();
                }
            });
        }
        false
    }

    /// The `denied` lines that the server did not write in this window
    /// so far.
    pub fn dropped(&self) -> u64 {
        self.window().dropped
    }

    /// Writes the line `dropped` of this window now, when it has a
    /// count. The server calls it when it stops, so no count is lost.
    pub fn close(&self) {
        self.window().close();
    }

    /// Writes the line `denied` of a call whose caller the server knows
    /// already, when the window has a line left.
    pub fn denied(&self, path: &str, named: Option<&Named>, code: DeniedCode) {
        if self.take() {
            denied(path, named, code);
        }
    }

    /// Writes the line `denied` of the call `request`, when the window
    /// has a line left. It reads the body for the name only then
    /// (01M3Z67B9RMVKY7TCXCG8HEZT4). It gives false when the read of the
    /// body stopped before its end: the reply then closes the call
    /// ([`Found::whole`]).
    pub async fn refuse(&self, request: Request, code: DeniedCode) -> bool {
        if !self.take() {
            return true;
        }
        let path = request.uri().path().to_owned();
        let found = Named::in_request(request).await;
        denied(&path, found.named.as_ref(), code);
        found.whole
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::logline::testing::{Capture, capture};
    use crate::state::{Caller, Code};

    fn traced() -> Traced {
        let me: SessionUri = "riff://mike@pangolin/como-technologies/riff?session=84cf"
            .parse()
            .unwrap();
        Traced {
            caller: Caller::of(&me).by(),
            key: Some("0f3a".into()),
            command: CommandKind::Claim,
        }
    }

    #[test]
    fn a_refused_command_gives_one_line_with_its_code_and_its_reason() {
        let lines = capture(|| {
            let refused = Refused::new(Code::Held, "issue-355 is held by mike/a6cf");
            traced().line(Outcome::Refused(&refused));
        });
        assert_eq!(lines.len(), 1);
        let line = &lines[0];
        assert_eq!(line["severity"], "INFO");
        assert_eq!(line["target"], "engine");
        assert_eq!(line["message"], "refused");
        assert_eq!(line["caller"], json!({"session": "mike/84cf"}));
        assert_eq!(line["key"], "0f3a");
        assert_eq!(line["command"], "claim");
        assert_eq!(line["result"], "refused");
        assert_eq!(line["code"], "held");
        assert_eq!(line["reason"], "issue-355 is held by mike/a6cf");
    }

    #[test]
    fn a_command_with_no_change_gives_one_line_with_no_code() {
        let lines = capture(|| {
            let traced = Traced {
                caller: By::Server,
                key: None,
                command: CommandKind::Forget,
            };
            traced.line(Outcome::NoChange);
        });
        assert_eq!(lines.len(), 1);
        let line = lines[0].as_object().unwrap();
        assert_eq!(line["severity"], "INFO");
        assert_eq!(line["caller"], "server");
        assert_eq!(line["command"], "forget");
        assert_eq!(line["result"], "no_change");
        for absent in ["key", "code", "reason"] {
            assert!(!line.contains_key(absent), "{absent}");
        }
    }

    #[test]
    fn a_command_whose_chunk_is_not_written_gives_a_line_with_the_severity_error() {
        let lines = capture(|| traced().line(Outcome::Failed));
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0]["severity"], "ERROR");
        assert_eq!(lines[0]["result"], "failed");
        assert_eq!(lines[0]["command"], "claim");
    }

    #[test]
    fn a_command_that_waits_at_a_stop_gives_a_line_with_the_severity_warning() {
        let lines = capture(|| traced().line(Outcome::Stopped("another instance holds the lease")));
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0]["severity"], "WARNING");
        assert_eq!(lines[0]["result"], "failed");
        assert_eq!(lines[0]["reason"], "another instance holds the lease");
        assert_eq!(lines[0]["command"], "claim");
    }

    #[test]
    fn a_denied_line_names_the_caller_as_not_proved_with_the_path_and_the_code() {
        let lines = capture(|| {
            let named = Named::of_text("riff://mike@pangolin/como-technologies/riff?session=84cf");
            denied("/v1/claim", Some(&named), DeniedCode::NotYou);
            denied("/v1/claim", None, DeniedCode::NoToken);
        });
        assert_eq!(lines.len(), 2);
        let line = lines[0].as_object().unwrap();
        assert_eq!(line["severity"], "INFO");
        assert_eq!(line["message"], "denied");
        assert_eq!(line["named"], json!({"session": "mike/84cf"}));
        assert_eq!(line["proved"], false);
        assert_eq!(line["path"], "/v1/claim");
        assert_eq!(line["result"], "denied");
        assert_eq!(line["code"], "not_you");
        for absent in ["caller", "command", "key", "reason", "named_cut"] {
            assert!(!line.contains_key(absent), "{absent}");
        }
        let line = lines[1].as_object().unwrap();
        assert_eq!(line["code"], "no_token");
        for absent in ["named", "proved", "named_cut"] {
            assert!(!line.contains_key(absent), "{absent}");
        }
    }

    #[test]
    fn a_person_is_named_by_the_user() {
        let named = Named::of_text("riff://mike@pangolin");
        assert_eq!(named.json(), &json!({"person": "mike"}));
    }

    #[test]
    fn a_long_name_and_a_name_with_a_line_break_stay_one_line_of_the_limit() {
        let long = "x".repeat(64 * 1024);
        let broken = "riff://mike@pangolin\n{\"severity\":\"ERROR\"}";
        let lines = capture(|| {
            denied(
                "/v1/claim",
                Some(&Named::of_text(&long)),
                DeniedCode::NoToken,
            );
            denied(
                "/v1/claim",
                Some(&Named::of_text(broken)),
                DeniedCode::NoToken,
            );
        });
        // `capture` reads each line of the output as one JSON object.
        assert_eq!(lines.len(), 2);
        let text = lines[0]["named"]["text"].as_str().unwrap();
        assert_eq!(text.chars().count(), NAMED_MAX);
        assert_eq!(lines[0]["named_cut"], true);
        assert!(lines[0].to_string().len() < 600, "{}", lines[0]);
        assert_eq!(lines[1]["named"]["text"], broken);
        assert_eq!(lines[1]["severity"], "INFO");
        assert!(!lines[1].as_object().unwrap().contains_key("named_cut"));
    }

    /// A window of the limit of rate (01M3Z67DZX9BC3TYF3PWGFGZJ7): the first
    /// [`DENIED_MAX`] lines, then one line with the count at its end.
    #[tokio::test(start_paused = true)]
    async fn a_window_has_the_lines_of_the_limit_and_one_line_with_the_count() {
        let capture = Capture::start();
        let limit = Limit::default();
        for _ in 0..1000 {
            limit.denied("/v1/claim", None, DeniedCode::NoToken);
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
        assert_eq!(capture.results("denied").len() as u64, DENIED_MAX);
        // The line with the count comes at the end of the window.
        assert!(capture.results("dropped").is_empty());
        tokio::time::sleep(DENIED_INTERVAL).await;
        let dropped = capture.results("dropped");
        assert_eq!(dropped.len(), 1, "{dropped:?}");
        let line = dropped[0].as_object().unwrap();
        assert_eq!(line["severity"], "WARNING");
        assert_eq!(line["target"], "engine");
        assert_eq!(line["message"], "dropped");
        assert_eq!(line["count"], 1000 - DENIED_MAX);
        for absent in ["named", "path", "code", "caller"] {
            assert!(!line.contains_key(absent), "{absent}");
        }

        // The next window starts clean. A window with no dropped line
        // gives no line with a count.
        limit.denied("/v1/claim", None, DeniedCode::NoToken);
        assert_eq!(capture.results("denied").len() as u64, DENIED_MAX + 1);
        tokio::time::sleep(DENIED_INTERVAL * 3).await;
        assert_eq!(capture.results("dropped").len(), 1);
    }

    /// The first call after a window writes the count of that window
    /// when its timer did not run yet, and the timer then writes
    /// nothing: one line for each window.
    #[tokio::test(start_paused = true)]
    async fn a_call_after_the_window_writes_its_count_one_time() {
        let capture = Capture::start();
        let limit = Limit::default();
        for _ in 0..DENIED_MAX + 7 {
            limit.denied("/v1/claim", None, DeniedCode::NoToken);
        }
        // The time moves, and the timer task does not run before the
        // next call.
        tokio::time::advance(DENIED_INTERVAL).await;
        limit.denied("/v1/claim", None, DeniedCode::NoToken);
        tokio::time::sleep(DENIED_INTERVAL * 3).await;
        let dropped = capture.results("dropped");
        assert_eq!(dropped.len(), 1, "{dropped:?}");
        assert_eq!(dropped[0]["count"], 7);
        assert_eq!(capture.results("denied").len() as u64, DENIED_MAX + 1);
    }

    /// A stop ends the window: `close` writes the count one time, also
    /// with no runtime.
    #[test]
    fn a_close_writes_the_count_of_the_window_one_time() {
        let lines = capture(|| {
            let limit = Limit::default();
            limit.close();
            for _ in 0..DENIED_MAX + 3 {
                limit.denied("/v1/claim", None, DeniedCode::OldBuild);
            }
            assert_eq!(limit.dropped(), 3);
            limit.close();
            limit.close();
        });
        assert_eq!(lines.len() as u64, DENIED_MAX + 1);
        let last = lines.last().unwrap();
        assert_eq!(last["result"], "dropped");
        assert_eq!(last["count"], 3);
    }

    fn request(body: axum::body::Body) -> Request {
        Request::builder()
            .method("POST")
            .uri("/v1/claim")
            .body(body)
            .unwrap()
    }

    /// A body that gives its start and then nothing.
    fn slow_body(start: &'static str) -> axum::body::Body {
        use tokio_stream::StreamExt;

        let first = tokio_stream::once(Ok::<_, std::convert::Infallible>(start));
        axum::body::Body::from_stream(first.chain(tokio_stream::pending()))
    }

    /// The read of a body that does not end stops at [`BODY_TIME`]
    /// (01M3Z67B9RMVKY7TCXCG8HEZT4), with no name.
    #[tokio::test(start_paused = true)]
    async fn the_read_of_a_slow_body_ends_at_the_time_limit_with_no_name() {
        let start = Instant::now();
        let found = Named::in_request(request(slow_body(r#"{"me":"riff://mike@pangolin"#))).await;
        assert_eq!(start.elapsed(), BODY_TIME);
        assert_eq!(
            found,
            Found {
                named: None,
                whole: false
            }
        );

        // A whole body gives its name, and a body past the limit of
        // size gives none.
        let whole = r#"{"me":"riff://mike@pangolin"}"#;
        let found = Named::in_request(request(whole.into())).await;
        assert_eq!(found.named.unwrap().json(), &json!({"person": "mike"}));
        assert!(found.whole);
        let long = "x".repeat(BODY_MAX + 1);
        let found = Named::in_request(request(long.into())).await;
        assert_eq!(
            found,
            Found {
                named: None,
                whole: false
            }
        );
    }

    /// Over the limit of rate, the server does not read the body: a
    /// call with a slow body ends at once.
    #[tokio::test(start_paused = true)]
    async fn a_call_over_the_limit_gets_no_read_of_its_body() {
        let capture = Capture::start();
        let limit = Limit::default();
        for _ in 0..DENIED_MAX {
            assert!(
                limit
                    .refuse(request("{}".into()), DeniedCode::NoToken)
                    .await
            );
        }
        let start = Instant::now();
        let slow = request(slow_body("{"));
        assert!(limit.refuse(slow, DeniedCode::NoToken).await);
        assert_eq!(start.elapsed(), Duration::ZERO);
        assert_eq!(limit.dropped(), 1);
        assert_eq!(capture.results("denied").len() as u64, DENIED_MAX);
    }

    #[test]
    fn each_denied_code_has_a_name_of_its_own() {
        let mut names: Vec<&str> = DeniedCode::ALL.iter().map(|code| code.as_str()).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), DeniedCode::ALL.len());
    }
}
