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
//! | `caller`, `named` | a caller of a call as JSON, for example `{"session":"mike/84cf"}` ([`crate::trace`]) |
//! | `counts` | the count of each code in a `dropped` line as JSON, for example `{"no_token":900}` ([`crate::trace`]) |
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

/// The fields whose value is JSON: a caller of a call, as a record
/// names it, and the counts of a `dropped` line ([`crate::trace`]).
/// The line holds the value as JSON, not as text. A value that is not
/// JSON stays text.
const JSON: [&str; 3] = ["caller", "named", "counts"];

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
            return;
        }
        // The event gives a caller as the text of its JSON.
        let value = match value {
            Value::String(text) if JSON.contains(&field.name()) => {
                serde_json::from_str(&text).unwrap_or(Value::String(text))
            }
            other => other,
        };
        self.fields.push((field.name().to_owned(), value));
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

/// The log lines of a test: the events of the thread of the test, in
/// the format of the server.
///
/// The test process has one subscriber for all its threads, as the
/// server has ([`init`]). It gives each line to the [`testing::Capture`]
/// of the thread that wrote it, and drops the line of a thread with no
/// capture.
///
/// A subscriber for each thread loses lines. `tracing` keeps for each
/// call site, for the whole process, whether a subscriber wants it.
/// With one subscriber, it asks only the thread that uses the call site
/// first. So a thread with no subscriber turned the call site off for
/// each thread.
#[cfg(test)]
pub(crate) mod testing {
    use std::cell::RefCell;
    use std::sync::{Arc, Mutex, Once};

    use serde_json::Value;

    use super::JsonLines;

    /// A writer that keeps each byte.
    #[derive(Clone, Default)]
    struct Kept(Arc<Mutex<Vec<u8>>>);

    thread_local! {
        /// The lines of the capture of this thread, when it has one.
        static KEPT: RefCell<Option<Kept>> = const { RefCell::new(None) };
    }

    /// The writer of one line: to the capture of this thread, or to
    /// nothing.
    struct ToThread(Option<Kept>);

    impl std::io::Write for ToThread {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if let Some(kept) = &self.0 {
                kept.0.lock().unwrap().extend_from_slice(bytes);
            }
            Ok(bytes.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    /// Starts the one subscriber of the test process, one time.
    fn subscribe() {
        static STARTED: Once = Once::new();
        STARTED.call_once(|| {
            let subscriber = tracing_subscriber::fmt()
                .event_format(JsonLines)
                // A thread that ends has no capture.
                .with_writer(|| {
                    ToThread(KEPT.try_with(|kept| kept.borrow().clone()).ok().flatten())
                })
                .finish();
            tracing::subscriber::set_global_default(subscriber)
                .expect("the test process has no other subscriber");
        });
    }

    /// Keeps each log line of this thread while it lives. An async test
    /// needs a runtime with one thread, so that each task writes here.
    /// A thread has at most one.
    pub(crate) struct Capture {
        kept: Kept,
    }

    impl Drop for Capture {
        fn drop(&mut self) {
            KEPT.with_borrow_mut(|kept| *kept = None);
        }
    }

    impl Capture {
        pub(crate) fn start() -> Capture {
            subscribe();
            // A call site that a thread used before the subscriber
            // started is off. This asks the subscriber again.
            tracing::callsite::rebuild_interest_cache();
            let kept = Kept::default();
            let old = KEPT.with_borrow_mut(|of_thread| of_thread.replace(kept.clone()));
            assert!(old.is_none(), "this thread has a capture");
            Capture { kept }
        }

        /// The text of the lines so far.
        pub(crate) fn text(&self) -> String {
            String::from_utf8(self.kept.0.lock().unwrap().clone()).unwrap()
        }

        /// Each line so far, as JSON. A line that is not one JSON object
        /// fails the test.
        pub(crate) fn lines(&self) -> Vec<Value> {
            let text = self.text();
            text.lines()
                .map(|line| {
                    let value: Value = serde_json::from_str(line)
                        .unwrap_or_else(|error| panic!("{error}: {line}"));
                    assert!(value.is_object(), "{line}");
                    value
                })
                .collect()
        }

        /// Each line so far with this `result`: the lines of a trace
        /// ([`crate::trace`]).
        pub(crate) fn results(&self, result: &str) -> Vec<Value> {
            let mut lines = self.lines();
            lines.retain(|line| line["result"] == result);
            lines
        }
    }

    /// The log lines of `run`.
    pub(crate) fn capture(run: impl FnOnce()) -> Vec<Value> {
        let capture = Capture::start();
        run();
        capture.lines()
    }
}

#[cfg(test)]
mod tests {
    use super::testing::{Capture, capture};

    #[test]
    fn a_field_with_a_caller_is_json_in_the_line() {
        let lines = capture(|| {
            tracing::info!(caller = r#"{"session":"mike/84cf"}"#, "refused");
            tracing::info!(named = "no json", other = r#"{"a":1}"#, "denied");
        });
        assert_eq!(lines[0]["caller"]["session"], "mike/84cf");
        assert_eq!(lines[1]["named"], "no json");
        // Each other field stays text.
        assert_eq!(lines[1]["other"], r#"{"a":1}"#);
    }

    /// One call site of a log line, for two threads.
    fn shared() {
        tracing::error!("a line of a call site that two threads use");
    }

    /// A thread with no capture that writes a line first does not hide
    /// that line from the capture of another thread
    /// (01M3X4Z62RJREQ5H8F18Y85T6V).
    #[test]
    fn a_thread_with_no_capture_does_not_hide_a_line_from_a_capture() {
        let capture = Capture::start();
        std::thread::spawn(shared).join().unwrap();
        shared();
        let lines = capture.lines();
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert_eq!(lines[0]["severity"], "ERROR");
    }

    #[test]
    fn each_event_is_one_line_of_json_with_a_severity() {
        let lines = capture(|| {
            tracing::info!(position = 7, name = "a\nb", "wrote the \"checkpoint\"");
            tracing::error!("riff-server stops: {}", "why");
            tracing::warn!(severity = "x", "a field with a name of the line");
        });
        assert_eq!(lines.len(), 3, "{lines:?}");
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
