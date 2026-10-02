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
//!     T -->|refused| D["line: denied<br/>named, path, code"]
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

use axum::extract::{Query, Request};
use riff_core::name::SessionUri;
use riff_core::record::By;
use serde_json::{Value, json};

use crate::state::{CommandKind, Refused};

/// The `target` of each line of a trace.
pub const TARGET: &str = "engine";

/// The most characters of a name in `named`. A longer name is cut.
pub const NAMED_MAX: usize = 200;

/// The most bytes of the body of a refused call that the server reads
/// to find the name.
pub const BODY_MAX: usize = 64 * 1024;

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
            // The reason of a command of the people can name an email:
            // its line has only the code (01M3XA87CJHCGZX283ZQAFKARZ).
            Outcome::Refused(refused) if self.command.of_people() => tracing::info!(
                target: TARGET,
                caller,
                key,
                command,
                result = "refused",
                code = refused.code.as_str(),
                "refused"
            ),
            Outcome::Refused(refused) => tracing::info!(
                target: TARGET,
                caller,
                key,
                command,
                result = "refused",
                code = refused.code.as_str(),
                reason = refused.reason.as_str(),
                "refused"
            ),
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
    /// most [`BODY_MAX`] bytes of the body. A call with a longer body,
    /// or with no name, gives `None`.
    pub async fn in_request(request: Request) -> Option<Named> {
        type Fields = std::collections::HashMap<String, String>;
        if let Ok(Query(query)) = Query::<Fields>::try_from_uri(request.uri())
            && let Some(me) = query.get("uri").or_else(|| query.get("me"))
        {
            return Some(Named::of_text(me));
        }
        let bytes = axum::body::to_bytes(request.into_body(), BODY_MAX)
            .await
            .ok()?;
        let body: Value = serde_json::from_slice(&bytes).ok()?;
        Some(Named::of_text(body.get("me")?.as_str()?))
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::logline::testing::capture;
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

    #[test]
    fn each_denied_code_has_a_name_of_its_own() {
        let mut names: Vec<&str> = DeniedCode::ALL.iter().map(|code| code.as_str()).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), DeniedCode::ALL.len());
    }
}
