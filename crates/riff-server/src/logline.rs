//! The log lines of `riff-server`: each line is one JSON object with a
//! `severity` field (01M3TJWJ3VK671T9NM95F3ES82).
//!
//! # Design
//!
//! Cloud Run sends each line of the output of the server to Cloud
//! Logging. Cloud Logging reads a line of JSON as a structured entry,
//! and takes its level from the field `severity`. So a filter
//! `severity>=ERROR` finds each error, and an alert on that filter goes
//! to the owner (01M3TJWJ6J3M6JRXJTAETZ5M6F).
//!
//! ```text
//! {"severity":"INFO","time":"2026-09-30T12:00:00.000Z","message":"riff-server listens on 127.0.0.1:7878","target":"riff_server"}
//! ```
//!
//! | Field | Holds |
//! |---|---|
//! | `severity` | `DEBUG`, `INFO`, `WARNING` or `ERROR` ([`severity`]) |
//! | `time` | the UTC time of the line |
//! | `message` | the text |
//! | `target` | the module that wrote the line |
//! | each other field | a value of the event, for example `position` |
//!
//! [`JsonLines`] is the event format for `tracing_subscriber`. [`init`]
//! starts the log of the process with it.
//!
//! # Example
//!
//! ```
//! use riff_server::logline::line;
//! use tracing::Level;
//!
//! let fields = [("position".to_owned(), serde_json::json!(7))];
//! let text = line(&Level::WARN, "2026-09-30T12:00:00.000Z", "riff_server", "a \"note\"", &fields);
//! assert_eq!(
//!     text,
//!     r#"{"severity":"WARNING","time":"2026-09-30T12:00:00.000Z","message":"a \"note\"","target":"riff_server","position":7}"#
//! );
//! let read: serde_json::Value = serde_json::from_str(&text).unwrap();
//! assert_eq!(read["severity"], "WARNING");
//! ```

use std::fmt;

use serde_json::Value;
use tracing::field::{Field, Visit};
use tracing::{Event, Level, Subscriber};
use tracing_subscriber::fmt::format::Writer;
use tracing_subscriber::fmt::{FmtContext, FormatEvent, FormatFields};
use tracing_subscriber::registry::LookupSpan;

/// The names that an event field cannot take: the line has them.
const OWN: [&str; 4] = ["severity", "time", "message", "target"];

/// The `severity` of Cloud Logging for a level.
///
/// ```
/// use riff_server::logline::severity;
/// use tracing::Level;
///
/// assert_eq!(severity(&Level::ERROR), "ERROR");
/// assert_eq!(severity(&Level::WARN), "WARNING");
/// assert_eq!(severity(&Level::TRACE), "DEBUG");
/// ```
pub fn severity(level: &Level) -> &'static str {
    match *level {
        Level::ERROR => "ERROR",
        Level::WARN => "WARNING",
        Level::INFO => "INFO",
        _ => "DEBUG",
    }
}

/// One log line: a JSON object with `severity`, `time`, `message` and
/// `target` first, then each field. A field with the name of one of
/// these four gets `_` at its end.
pub fn line(
    level: &Level,
    time: &str,
    target: &str,
    message: &str,
    fields: &[(String, Value)],
) -> String {
    let text = |value: &str| Value::from(value).to_string();
    let mut line = format!(
        "{{\"severity\":{},\"time\":{},\"message\":{},\"target\":{}",
        text(severity(level)),
        text(time),
        text(message),
        text(target)
    );
    for (name, value) in fields {
        let name = if OWN.contains(&name.as_str()) {
            format!("{name}_")
        } else {
            name.clone()
        };
        line.push_str(&format!(",{}:{value}", text(&name)));
    }
    line.push('}');
    line
}

/// The event format of `riff-server`: one [`line()`] for each event.
#[derive(Clone, Copy, Debug, Default)]
pub struct JsonLines;

/// Collects the message and the fields of one event.
#[derive(Default)]
struct Fields {
    message: String,
    fields: Vec<(String, Value)>,
}

impl Fields {
    fn put(&mut self, field: &Field, value: Value) {
        if field.name() == "message" {
            self.message = match value {
                Value::String(text) => text,
                other => other.to_string(),
            };
        } else {
            self.fields.push((field.name().to_owned(), value));
        }
    }
}

impl Visit for Fields {
    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        self.put(field, Value::from(format!("{value:?}")));
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        self.put(field, Value::from(value));
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        self.put(field, Value::from(value));
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        self.put(field, Value::from(value));
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        self.put(field, Value::from(value));
    }

    fn record_f64(&mut self, field: &Field, value: f64) {
        self.put(field, Value::from(value));
    }
}

impl<S, N> FormatEvent<S, N> for JsonLines
where
    S: Subscriber + for<'a> LookupSpan<'a>,
    N: for<'a> FormatFields<'a> + 'static,
{
    fn format_event(
        &self,
        _ctx: &FmtContext<'_, S, N>,
        mut writer: Writer<'_>,
        event: &Event<'_>,
    ) -> fmt::Result {
        let mut fields = Fields::default();
        event.record(&mut fields);
        let time = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        let meta = event.metadata();
        let text = line(
            meta.level(),
            &time,
            meta.target(),
            &fields.message,
            &fields.fields,
        );
        writeln!(writer, "{text}")
    }
}

/// Starts the log of the process: each event is one [`line()`] on stdout.
/// `RUST_LOG` sets the levels. The default is `info`.
pub fn init() {
    tracing_subscriber::fmt()
        .event_format(JsonLines)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    /// A writer that keeps each byte, for a test.
    #[derive(Clone, Default)]
    struct Kept(Arc<Mutex<Vec<u8>>>);

    impl std::io::Write for Kept {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn each_event_is_one_line_of_json_with_a_severity() {
        let kept = Kept::default();
        let writer = kept.clone();
        let subscriber = tracing_subscriber::fmt()
            .event_format(JsonLines)
            .with_writer(move || writer.clone())
            .finish();
        tracing::subscriber::with_default(subscriber, || {
            tracing::info!(position = 7, name = "a\nb", "wrote the \"checkpoint\"");
            tracing::error!("riff-server stops: {}", "why");
            tracing::warn!(severity = "x", "a field with a name of the line");
        });
        let text = String::from_utf8(kept.0.lock().unwrap().clone()).unwrap();
        let lines: Vec<Value> = text
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(lines.len(), 3, "{text}");
        assert_eq!(lines[0]["severity"], "INFO");
        assert_eq!(lines[0]["message"], "wrote the \"checkpoint\"");
        assert_eq!(lines[0]["position"], 7);
        assert_eq!(lines[0]["name"], "a\nb");
        assert!(lines[0]["time"].as_str().unwrap().ends_with('Z'));
        assert_eq!(lines[1]["severity"], "ERROR");
        assert_eq!(lines[1]["message"], "riff-server stops: why");
        assert_eq!(lines[2]["severity"], "WARNING");
        assert_eq!(lines[2]["severity_"], "x");
    }
}
